package id.inoerawan.redbx

// The `Redbx*` types below are UniFFI-generated and live in this same package,
// so they need no import. This file wraps them in a scoped, coroutine-friendly
// API; the generated types remain available for callers that want them directly.
import java.io.File
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * An encrypted, embedded key-value database.
 *
 * Every operation touches the filesystem and is therefore blocking. The suspend
 * functions ([write], [read], [compact]) move that work to [Dispatchers.IO]; the
 * `*Blocking` variants are provided for callers that already run off the main
 * thread. Calling a `*Blocking` variant on the main thread will cause an ANR.
 *
 * Handles are reference-counted across the FFI boundary and are **not** released
 * by the garbage collector in a timely way. The scoped [write] / [read] functions
 * exist so that transactions and table handles are always released — including on
 * failure. A write transaction that is never released keeps redbx's single write
 * slot held, which stalls every subsequent write.
 *
 * The database itself is closed with [close]; prefer `use { }`.
 *
 * ```kotlin
 * Redbx.open(context.getDatabasePath("app.redbx"), password).use { db ->
 *     db.write { txn ->
 *         txn.table("settings").insert("theme".toRedbx(), "dark".toRedbx())
 *     }
 *     val theme = db.read { txn ->
 *         txn.table("settings").get("theme".toRedbx())?.asStringOrNull()
 *     }
 * }
 * ```
 */
public class Redbx internal constructor(
    private val db: RedbxDatabase,
    private val dispatcher: CoroutineDispatcher,
) : AutoCloseable {

    public companion object {
        /**
         * Create a new encrypted database at [path], protected by [password].
         *
         * Fails if the file exists but is not a valid redbx database.
         */
        public suspend fun create(
            path: String,
            password: String,
            dispatcher: CoroutineDispatcher = Dispatchers.IO,
        ): Redbx = withContext(dispatcher) { createBlocking(path, password, dispatcher) }

        public suspend fun create(
            file: File,
            password: String,
            dispatcher: CoroutineDispatcher = Dispatchers.IO,
        ): Redbx = create(file.absolutePath, password, dispatcher)

        /** Open an existing encrypted database. Throws `RedbxException.IncorrectPassword` on a bad password. */
        public suspend fun open(
            path: String,
            password: String,
            dispatcher: CoroutineDispatcher = Dispatchers.IO,
        ): Redbx = withContext(dispatcher) { openBlocking(path, password, dispatcher) }

        public suspend fun open(
            file: File,
            password: String,
            dispatcher: CoroutineDispatcher = Dispatchers.IO,
        ): Redbx = open(file.absolutePath, password, dispatcher)

        @JvmStatic
        @JvmOverloads
        public fun createBlocking(
            path: String,
            password: String,
            dispatcher: CoroutineDispatcher = Dispatchers.IO,
        ): Redbx {
            val file = File(path)
            file.parentFile?.mkdirs()
            return Redbx(RedbxDatabase.create(path, password), dispatcher)
        }

        @JvmStatic
        @JvmOverloads
        public fun openBlocking(
            path: String,
            password: String,
            dispatcher: CoroutineDispatcher = Dispatchers.IO,
        ): Redbx = Redbx(RedbxDatabase.open(path, password), dispatcher)
    }

    /**
     * Run [block] inside a write transaction, committing when it returns normally
     * and aborting if it throws. Every table handle opened through the supplied
     * [WriteTransaction] is released before the transaction ends.
     *
     * Do not let a [Table] escape [block] — its handle is released when the block
     * ends, so using it afterwards throws [IllegalStateException].
     */
    public suspend fun <T> write(block: (WriteTransaction) -> T): T =
        withContext(dispatcher) { writeBlocking(block) }

    /** Blocking form of [write]. Never call this on the main thread. */
    public fun <T> writeBlocking(block: (WriteTransaction) -> T): T {
        val txn = db.beginWrite()
        val scope = WriteTransaction(txn)
        try {
            val result = block(scope)
            scope.releaseTables()
            txn.commit()
            return result
        } catch (t: Throwable) {
            scope.releaseTables()
            // The transaction may already be consumed (e.g. commit() threw); abort
            // is a no-op in that case, and must not mask the original failure.
            runCatching { txn.abort() }
            throw t
        } finally {
            txn.close()
        }
    }

    /**
     * Run [block] against a read-only snapshot. Multiple readers may run
     * concurrently, and readers are not blocked by an in-flight write.
     */
    public suspend fun <T> read(block: (ReadTransaction) -> T): T =
        withContext(dispatcher) { readBlocking(block) }

    /** Blocking form of [read]. Never call this on the main thread. */
    public fun <T> readBlocking(block: (ReadTransaction) -> T): T {
        val txn = db.beginRead()
        val scope = ReadTransaction(txn)
        try {
            return block(scope)
        } finally {
            scope.releaseTables()
            txn.close()
        }
    }

    /**
     * Reclaim pages freed by earlier deletions. Returns `true` if anything moved.
     *
     * Takes exclusive access to the database: no transaction may be in flight, and
     * none can start while it runs. This can be slow on a large file.
     */
    public suspend fun compact(): Boolean = withContext(dispatcher) { compactBlocking() }

    /** Blocking form of [compact]. Never call this on the main thread. */
    public fun compactBlocking(): Boolean = db.compact()

    /** Release the underlying database handle. Idempotent. */
    override fun close() {
        db.close()
    }
}

/**
 * Table handles opened during a write transaction.
 *
 * Instances are only valid for the duration of the enclosing [Redbx.write] block.
 */
public class WriteTransaction internal constructor(private val txn: RedbxWriteTransaction) {
    private val opened = ArrayList<AutoCloseable>()

    /** Open (creating if absent) a key-value table. */
    public fun table(name: String): Table = Table(txn.openTable(name)).also { opened.add(it) }

    /** Open (creating if absent) a table where one key may hold many values. */
    public fun multimapTable(name: String): MultimapTable =
        MultimapTable(txn.openMultimapTable(name)).also { opened.add(it) }

    internal fun releaseTables() {
        for (i in opened.indices.reversed()) {
            runCatching { opened[i].close() }
        }
        opened.clear()
    }
}

/**
 * Table handles opened during a read transaction.
 *
 * Instances are only valid for the duration of the enclosing [Redbx.read] block.
 */
public class ReadTransaction internal constructor(private val txn: RedbxReadTransaction) {
    private val opened = ArrayList<AutoCloseable>()

    public fun table(name: String): ReadOnlyTable =
        ReadOnlyTable(txn.openTable(name)).also { opened.add(it) }

    public fun multimapTable(name: String): ReadOnlyMultimapTable =
        ReadOnlyMultimapTable(txn.openMultimapTable(name)).also { opened.add(it) }

    internal fun releaseTables() {
        for (i in opened.indices.reversed()) {
            runCatching { opened[i].close() }
        }
        opened.clear()
    }
}

/** A read-write key-value table. */
public class Table internal constructor(private val handle: RedbxTable) : AutoCloseable {
    /** Insert [key], replacing any existing value. */
    public fun insert(key: RedbxValue, value: RedbxValue): Unit = handle.insert(key, value)

    public fun get(key: RedbxValue): RedbxValue? = handle.get(key)

    /** Remove [key], returning the value it held. */
    public fun remove(key: RedbxValue): RedbxValue? = handle.remove(key)

    /**
     * Entries with keys in `[start, end]`, ascending.
     *
     * Both endpoints must be the same [RedbxValue] variant, otherwise
     * `RedbxException.InvalidRange` is thrown.
     *
     * The full result is materialised in memory — bound the range on large tables.
     */
    public fun range(start: RedbxValue, end: RedbxValue): List<RedbxKeyValue> =
        handle.range(start, end)

    public fun count(): Long = handle.len().toLong()

    public fun isEmpty(): Boolean = handle.isEmpty()

    override fun close(): Unit = handle.close()
}

/** A read-only key-value table. */
public class ReadOnlyTable internal constructor(private val handle: RedbxReadOnlyTable) :
    AutoCloseable {
    public fun get(key: RedbxValue): RedbxValue? = handle.get(key)

    /** See [Table.range] for the endpoint and memory constraints. */
    public fun range(start: RedbxValue, end: RedbxValue): List<RedbxKeyValue> =
        handle.range(start, end)

    public fun count(): Long = handle.len().toLong()

    public fun isEmpty(): Boolean = handle.isEmpty()

    override fun close(): Unit = handle.close()
}

/** A read-write table where a key may hold many distinct values. */
public class MultimapTable internal constructor(private val handle: RedbxMultimapTable) :
    AutoCloseable {
    /** Add [value] under [key]. A duplicate pair is a no-op. */
    public fun insert(key: RedbxValue, value: RedbxValue): Unit = handle.insert(key, value)

    public fun get(key: RedbxValue): List<RedbxValue> = handle.get(key)

    /** Remove one specific pair. Returns `true` if it existed. */
    public fun remove(key: RedbxValue, value: RedbxValue): Boolean = handle.remove(key, value)

    /** Remove every value under [key]. Returns how many were removed. */
    public fun removeAll(key: RedbxValue): Long = handle.removeAll(key).toLong()

    /** See [Table.range] for the endpoint and memory constraints. */
    public fun range(start: RedbxValue, end: RedbxValue): List<RedbxKeyValue> =
        handle.range(start, end)

    override fun close(): Unit = handle.close()
}

/** A read-only multimap table. */
public class ReadOnlyMultimapTable internal constructor(
    private val handle: RedbxReadOnlyMultimapTable,
) : AutoCloseable {
    public fun get(key: RedbxValue): List<RedbxValue> = handle.get(key)

    /** See [Table.range] for the endpoint and memory constraints. */
    public fun range(start: RedbxValue, end: RedbxValue): List<RedbxKeyValue> =
        handle.range(start, end)

    override fun close(): Unit = handle.close()
}
