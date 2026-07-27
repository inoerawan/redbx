package id.inoerawan.redbx

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * On-device tests. These are the only tests that exercise the real .so on real
 * Android ABIs — the Rust unit tests run on the host and cannot catch packaging,
 * JNA loading, or ABI problems.
 *
 * Run with: `cd android && ./gradlew :redbx:connectedAndroidTest`
 */
@RunWith(AndroidJUnit4::class)
class RedbxInstrumentedTest {

    private lateinit var dir: File

    @Before
    fun setUp() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        dir = File(context.cacheDir, "redbx-test-${System.nanoTime()}").apply { mkdirs() }
    }

    @After
    fun tearDown() {
        dir.deleteRecursively()
    }

    private fun dbPath(name: String = "test.redbx") = File(dir, name).absolutePath

    // ── Library loading and lifecycle ─────────────────────────────────────────

    @Test
    fun createsAndReopensDatabase() = runBlocking {
        val path = dbPath()
        Redbx.create(path, "pass").use { db ->
            db.write { txn -> txn.table("kv").insert("k".toRedbx(), "v".toRedbx()) }
        }
        Redbx.open(path, "pass").use { db ->
            val value = db.read { txn -> txn.table("kv").get("k".toRedbx())?.asStringOrNull() }
            assertEquals("v", value)
        }
    }

    @Test
    fun wrongPasswordIsRejected() = runBlocking {
        val path = dbPath()
        Redbx.create(path, "correct").use { db ->
            db.write { txn -> txn.table("kv").insert(1L.toRedbx(), 1L.toRedbx()) }
        }
        val thrown = assertThrows(RedbxException.IncorrectPassword::class.java) {
            Redbx.openBlocking(path, "wrong")
        }
        assertNotNull(thrown)
    }

    // ── Values ───────────────────────────────────────────────────────────────

    @Test
    fun roundTripsEveryValueVariant() = runBlocking {
        Redbx.create(dbPath(), "pass").use { db ->
            db.write { txn ->
                val t = txn.table("types")
                t.insert(0L.toRedbx(), "text".toRedbx())
                t.insert(1L.toRedbx(), byteArrayOf(1, 2, 3).toRedbx())
                t.insert(2L.toRedbx(), true.toRedbx())
                t.insert(3L.toRedbx(), Int.MIN_VALUE.toRedbx())
                t.insert(4L.toRedbx(), Long.MAX_VALUE.toRedbx())
                t.insert(5L.toRedbx(), ULong.MAX_VALUE.toRedbx())
                t.insert(6L.toRedbx(), 1.5.toRedbx())
                t.insert(7L.toRedbx(), 2.5f.toRedbx())
            }
            db.read { txn ->
                val t = txn.table("types")
                assertEquals("text", t.get(0L.toRedbx())?.asStringOrNull())
                    assertArrayEquals(byteArrayOf(1, 2, 3), t.get(1L.toRedbx())?.asByteArrayOrNull())
                assertEquals(true, t.get(2L.toRedbx())?.asBooleanOrNull())
                assertEquals(Int.MIN_VALUE, t.get(3L.toRedbx())?.asIntOrNull())
                assertEquals(Long.MAX_VALUE, t.get(4L.toRedbx())?.asLongOrNull())
                assertEquals(ULong.MAX_VALUE, t.get(5L.toRedbx())?.asULongOrNull())
                assertEquals(1.5, t.get(6L.toRedbx())?.asDoubleOrNull())
                assertEquals(2.5f, t.get(7L.toRedbx())?.asFloatOrNull())
            }
        }
    }

    @Test
    fun missingKeyReturnsNull() = runBlocking {
        Redbx.create(dbPath(), "pass").use { db ->
            db.write { txn -> txn.table("kv").insert("seed".toRedbx(), 0L.toRedbx()) }
            db.read { txn -> assertNull(txn.table("kv").get("absent".toRedbx())) }
        }
    }

    // ── Key ordering ─────────────────────────────────────────────────────────

    /**
     * Regression: keys were once encoded little-endian, which made byte-ordered
     * range scans return the wrong rows. Only values that cross a byte boundary
     * expose it.
     */
    @Test
    fun rangeIsOrderedAcrossByteBoundaries() = runBlocking {
        val keys = listOf(1L, 2L, 100L, 255L, 256L, 257L, 1000L, 70_000L)
        Redbx.create(dbPath(), "pass").use { db ->
            db.write { txn ->
                val t = txn.table("kv")
                keys.forEach { t.insert(it.toRedbx(), it.toRedbx()) }
            }
            db.read { txn ->
                val scanned = txn.table("kv")
                    .range(Long.MIN_VALUE.toRedbx(), Long.MAX_VALUE.toRedbx())
                    .map { it.key.asLongOrNull() }
                assertEquals(keys, scanned)

                val bounded = txn.table("kv")
                    .range(1L.toRedbx(), 300L.toRedbx())
                    .map { it.key.asLongOrNull() }
                assertEquals(listOf(1L, 2L, 100L, 255L, 256L, 257L), bounded)
            }
        }
    }

    @Test
    fun negativeKeysSortBeforePositive() = runBlocking {
        val keys = listOf(Long.MIN_VALUE, -70_000L, -1L, 0L, 1L, Long.MAX_VALUE)
        Redbx.create(dbPath(), "pass").use { db ->
            db.write { txn ->
                val t = txn.table("kv")
                keys.forEach { t.insert(it.toRedbx(), it.toRedbx()) }
            }
            db.read { txn ->
                val scanned = txn.table("kv")
                    .range(Long.MIN_VALUE.toRedbx(), Long.MAX_VALUE.toRedbx())
                    .map { it.key.asLongOrNull() }
                assertEquals(keys, scanned)
            }
        }
    }

    @Test
    fun mixedVariantRangeIsRejected() = runBlocking {
        Redbx.create(dbPath(), "pass").use { db ->
            val thrown = assertThrows(RedbxException.InvalidRange::class.java) {
                db.writeBlocking { txn ->
                    txn.table("kv").range(1.toRedbx(), 10L.toRedbx())
                }
            }
            assertNotNull(thrown)
        }
    }

    // ── Transactions ─────────────────────────────────────────────────────────

    @Test
    fun writeCommitsOnSuccess() = runBlocking {
        Redbx.create(dbPath(), "pass").use { db ->
            db.write { txn -> txn.table("kv").insert("k".toRedbx(), 1L.toRedbx()) }
            val count = db.read { txn -> txn.table("kv").count() }
            assertEquals(1L, count)
        }
    }

    @Test
    fun writeRollsBackWhenBlockThrows() = runBlocking {
        Redbx.create(dbPath(), "pass").use { db ->
            db.write { txn -> txn.table("kv").insert("seed".toRedbx(), 0L.toRedbx()) }

            val boom = assertThrows(IllegalStateException::class.java) {
                db.writeBlocking { txn ->
                    txn.table("kv").insert("rolled-back".toRedbx(), 1L.toRedbx())
                    throw IllegalStateException("boom")
                }
            }
            assertEquals("boom", boom.message)

            db.read { txn ->
                val t = txn.table("kv")
                assertNull(t.get("rolled-back".toRedbx()))
                assertEquals(1L, t.count())
            }
        }
    }

    /**
     * The write slot is single-occupancy. If [Redbx.write] failed to release the
     * transaction, this second write would hang rather than fail — so a timeout
     * here means the scoping is broken.
     */
    @Test(timeout = 30_000)
    fun writeSlotIsReleasedForTheNextWrite() = runBlocking {
        Redbx.create(dbPath(), "pass").use { db ->
            repeat(5) { i ->
                db.write { txn -> txn.table("kv").insert(i.toLong().toRedbx(), i.toLong().toRedbx()) }
            }
            runCatching {
                db.write { txn ->
                    txn.table("kv").insert(99L.toRedbx(), 99L.toRedbx())
                    error("abort this one")
                }
            }
            db.write { txn -> txn.table("kv").insert(6L.toRedbx(), 6L.toRedbx()) }
            assertEquals(6L, db.read { txn -> txn.table("kv").count() })
        }
    }

    /**
     * A [Table] that escapes its `write` block is already released — [Redbx.write]
     * closes every handle it opened. Using it therefore fails at the FFI boundary
     * with [IllegalStateException], without an FFI call ever reaching Rust.
     *
     * (Rust's own `TransactionConsumed` path — handle still alive, transaction
     * gone — is covered by the host test `test_ops_after_commit_return_error`.)
     */
    @Test
    fun tableIsUnusableAfterItsTransactionEnds() = runBlocking {
        Redbx.create(dbPath(), "pass").use { db ->
            val escaped = db.write { txn ->
                txn.table("kv").also { it.insert(1L.toRedbx(), 1L.toRedbx()) }
            }
            val thrown = assertThrows(IllegalStateException::class.java) {
                escaped.insert(2L.toRedbx(), 2L.toRedbx())
            }
            assertTrue(
                "unexpected message: ${thrown.message}",
                thrown.message.orEmpty().contains("already been destroyed"),
            )
        }
    }

    // ── Multimap ─────────────────────────────────────────────────────────────

    @Test
    fun multimapHoldsManyValuesPerKey() = runBlocking {
        Redbx.create(dbPath(), "pass").use { db ->
            db.write { txn ->
                val t = txn.multimapTable("tags")
                t.insert("post".toRedbx(), "rust".toRedbx())
                t.insert("post".toRedbx(), "android".toRedbx())
                t.insert("post".toRedbx(), "database".toRedbx())
            }
            db.read { txn ->
                val values = txn.multimapTable("tags").get("post".toRedbx())
                assertEquals(3, values.size)
                assertEquals(
                    listOf("android", "database", "rust"),
                    values.mapNotNull { it.asStringOrNull() }.sorted(),
                )
            }
            db.write { txn ->
                val t = txn.multimapTable("tags")
                assertTrue(t.remove("post".toRedbx(), "rust".toRedbx()))
                assertEquals(2L, t.removeAll("post".toRedbx()))
            }
            db.read { txn ->
                assertTrue(txn.multimapTable("tags").get("post".toRedbx()).isEmpty())
            }
        }
    }

    // ── Maintenance ──────────────────────────────────────────────────────────

    @Test
    fun compactRunsOnAPopulatedDatabase() = runBlocking {
        Redbx.create(dbPath(), "pass").use { db ->
            db.write { txn ->
                val t = txn.table("kv")
                repeat(200) { t.insert(it.toLong().toRedbx(), it.toLong().toRedbx()) }
            }
            db.write { txn ->
                val t = txn.table("kv")
                repeat(200) { t.remove(it.toLong().toRedbx()) }
            }
            db.compact()
            assertTrue(db.read { txn -> txn.table("kv").isEmpty() })
        }
    }

    @Test
    fun emptyTableReportsEmpty() = runBlocking {
        Redbx.create(dbPath(), "pass").use { db ->
            db.write { txn ->
                val t = txn.table("kv")
                assertTrue(t.isEmpty())
                assertEquals(0L, t.count())
                t.insert(1L.toRedbx(), 1L.toRedbx())
                assertFalse(t.isEmpty())
            }
        }
    }
}
