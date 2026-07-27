@file:JvmName("RedbxValues")

package id.inoerawan.redbx

// `RedbxValue`, `RedbxKeyValue` and `RedbxException` are UniFFI-generated and
// live in this same package (see crates/redbx-mobile/uniffi.toml), so callers
// only ever import `id.inoerawan.redbx.*`.
//
// Keys are ordered by variant first, then by value. Two different variants never
// interleave — `U8(5)` and `U64(5)` are not adjacent — so a range query must use
// the same variant for both endpoints. Within a variant, ordering is the natural
// one: negative integers sort before positive, and floats follow IEEE-754
// `totalOrder`.

// ── Kotlin value → RedbxValue ─────────────────────────────────────────────────
//
// The variant fixes both the stored encoding and the sort order, so pick the one
// that matches how the data will be queried — storing a number as `Str` will sort
// it lexicographically ("10" before "9").

public fun String.toRedbx(): RedbxValue = RedbxValue.Str(this)

public fun ByteArray.toRedbx(): RedbxValue = RedbxValue.Bytes(this)

public fun Boolean.toRedbx(): RedbxValue = RedbxValue.Bool(this)

public fun Byte.toRedbx(): RedbxValue = RedbxValue.I8(this)

public fun Short.toRedbx(): RedbxValue = RedbxValue.I16(this)

public fun Int.toRedbx(): RedbxValue = RedbxValue.I32(this)

public fun Long.toRedbx(): RedbxValue = RedbxValue.I64(this)

public fun UByte.toRedbx(): RedbxValue = RedbxValue.U8(this)

public fun UShort.toRedbx(): RedbxValue = RedbxValue.U16(this)

public fun UInt.toRedbx(): RedbxValue = RedbxValue.U32(this)

public fun ULong.toRedbx(): RedbxValue = RedbxValue.U64(this)

public fun Float.toRedbx(): RedbxValue = RedbxValue.F32(this)

public fun Double.toRedbx(): RedbxValue = RedbxValue.F64(this)

// ── RedbxValue → Kotlin value ─────────────────────────────────────────────────
//
// These return null on a variant mismatch rather than throwing, so a caller can
// decide whether a wrong-typed row is an error or something to skip.

public fun RedbxValue.asStringOrNull(): String? = (this as? RedbxValue.Str)?.v1

public fun RedbxValue.asByteArrayOrNull(): ByteArray? = (this as? RedbxValue.Bytes)?.v1

public fun RedbxValue.asBooleanOrNull(): Boolean? = (this as? RedbxValue.Bool)?.v1

public fun RedbxValue.asByteOrNull(): Byte? = (this as? RedbxValue.I8)?.v1

public fun RedbxValue.asShortOrNull(): Short? = (this as? RedbxValue.I16)?.v1

public fun RedbxValue.asIntOrNull(): Int? = (this as? RedbxValue.I32)?.v1

public fun RedbxValue.asLongOrNull(): Long? = (this as? RedbxValue.I64)?.v1

public fun RedbxValue.asUByteOrNull(): UByte? = (this as? RedbxValue.U8)?.v1

public fun RedbxValue.asUShortOrNull(): UShort? = (this as? RedbxValue.U16)?.v1

public fun RedbxValue.asUIntOrNull(): UInt? = (this as? RedbxValue.U32)?.v1

public fun RedbxValue.asULongOrNull(): ULong? = (this as? RedbxValue.U64)?.v1

public fun RedbxValue.asFloatOrNull(): Float? = (this as? RedbxValue.F32)?.v1

public fun RedbxValue.asDoubleOrNull(): Double? = (this as? RedbxValue.F64)?.v1
