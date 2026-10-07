//! Core-SQL SHA-2 for DuckDB algorithms absent from its built-in catalog.
//! FIPS 180-4 round constants and initial states; all arithmetic is explicitly
//! modulo the word size. No extension or host callback is required.
const K32: [u64; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];
const K64: [u64; 80] = [
    0x428a2f98d728ae22,
    0x7137449123ef65cd,
    0xb5c0fbcfec4d3b2f,
    0xe9b5dba58189dbbc,
    0x3956c25bf348b538,
    0x59f111f1b605d019,
    0x923f82a4af194f9b,
    0xab1c5ed5da6d8118,
    0xd807aa98a3030242,
    0x12835b0145706fbe,
    0x243185be4ee4b28c,
    0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f,
    0x80deb1fe3b1696b1,
    0x9bdc06a725c71235,
    0xc19bf174cf692694,
    0xe49b69c19ef14ad2,
    0xefbe4786384f25e3,
    0x0fc19dc68b8cd5b5,
    0x240ca1cc77ac9c65,
    0x2de92c6f592b0275,
    0x4a7484aa6ea6e483,
    0x5cb0a9dcbd41fbd4,
    0x76f988da831153b5,
    0x983e5152ee66dfab,
    0xa831c66d2db43210,
    0xb00327c898fb213f,
    0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2,
    0xd5a79147930aa725,
    0x06ca6351e003826f,
    0x142929670a0e6e70,
    0x27b70a8546d22ffc,
    0x2e1b21385c26c926,
    0x4d2c6dfc5ac42aed,
    0x53380d139d95b3df,
    0x650a73548baf63de,
    0x766a0abb3c77b2a8,
    0x81c2c92e47edaee6,
    0x92722c851482353b,
    0xa2bfe8a14cf10364,
    0xa81a664bbc423001,
    0xc24b8b70d0f89791,
    0xc76c51a30654be30,
    0xd192e819d6ef5218,
    0xd69906245565a910,
    0xf40e35855771202a,
    0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8,
    0x1e376c085141ab53,
    0x2748774cdf8eeb99,
    0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63,
    0x4ed8aa4ae3418acb,
    0x5b9cca4f7763e373,
    0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc,
    0x78a5636f43172f60,
    0x84c87814a1f0ab72,
    0x8cc702081a6439ec,
    0x90befffa23631e28,
    0xa4506cebde82bde9,
    0xbef9a3f7b2c67915,
    0xc67178f2e372532b,
    0xca273eceea26619c,
    0xd186b8c721c0c207,
    0xeada7dd6cde0eb1e,
    0xf57d4f7fee6ed178,
    0x06f067aa72176fba,
    0x0a637dc5a2c898a6,
    0x113f9804bef90dae,
    0x1b710b35131c471b,
    0x28db77f523047d84,
    0x32caab7b40c72493,
    0x3c9ebe0a15c9bebc,
    0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6,
    0x597f299cfc657e2a,
    0x5fcb6fab3ad6faec,
    0x6c44198c4a475817,
];
const H256_224: [u64; 8] = [
    0xc1059ed8, 0x367cd507, 0x3070dd17, 0xf70e5939, 0xffc00b31, 0x68581511, 0x64f98fa7, 0xbefa4fa4,
];
const H512_384: [u64; 8] = [
    0xcbbb9d5dc1059ed8,
    0x629a292a367cd507,
    0x9159015a3070dd17,
    0x152fecd8f70e5939,
    0x67332667ffc00b31,
    0x8eb44a8768581511,
    0xdb0c2e0d64f98fa7,
    0x47b5481dbefa4fa4,
];
const H512_512: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];
fn array(values: &[u64]) -> String {
    format!(
        "CAST(ARRAY[{}] AS UHUGEINT[])",
        values
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(",")
    )
}
pub(super) fn mapping(name: &str, pg: bool) -> Option<String> {
    if pg {
        return None;
    }
    let (bits, output, initial, constants): (u32, usize, &[u64], &[u64]) = match name {
        "sha224" => (32, 56, &H256_224, &K32),
        "sha384" => (64, 96, &H512_384, &K64),
        "sha512" => (64, 128, &H512_512, &K64),
        _ => return None,
    };
    let mask = ((1u128 << bits) - 1).to_string();
    let rounds = constants.len();
    let hexword = bits / 4;
    let block = hexword * 16;
    let length = hexword * 2;
    let rotate = |s: &str, n: u32| format!("(({s} >> {n}) | (({s} << {}) & {mask}))", bits - n);
    let sigma = |s: &str, a, b, c, shift| {
        format!(
            "xor(xor({},{}),{})",
            rotate(s, a),
            rotate(s, b),
            if shift {
                format!("({s} >> {c})")
            } else {
                rotate(s, c)
            }
        )
    };
    let small0 = if bits == 32 {
        sigma("__local13[__local12-14]", 7, 18, 3, true)
    } else {
        sigma("__local13[__local12-14]", 1, 8, 7, true)
    };
    let small1 = if bits == 32 {
        sigma("__local13[__local12-1]", 17, 19, 10, true)
    } else {
        sigma("__local13[__local12-1]", 19, 61, 6, true)
    };
    let big0 = if bits == 32 {
        sigma("__local15[1]", 2, 13, 22, false)
    } else {
        sigma("__local15[1]", 28, 34, 39, false)
    };
    let big1 = if bits == 32 {
        sigma("__local15[5]", 6, 11, 25, false)
    } else {
        sigma("__local15[5]", 14, 18, 41, false)
    };
    let ch = format!(
        "xor((__local15[5] & __local15[6]), (xor(__local15[5], {mask}::UHUGEINT) & __local15[7]))"
    );
    let maj = "xor(xor((__local15[1] & __local15[2]), (__local15[1] & __local15[3])), (__local15[2] & __local15[3]))";
    let initial = array(initial);
    let constants = array(constants);
    let words = |offset: &str| {
        format!(
            "list_transform(range(0,16), lambda __local40: CAST(CAST('0x' || substr(__local43, CAST(({offset})*{block}+__local40*{hexword}+1 AS BIGINT), {hexword}) AS UBIGINT) AS UHUGEINT))"
        )
    };
    let first = words("0");
    // The last iteration does not need another message block. Avoid parsing
    // an empty hexadecimal word even if the optimizer evaluates the branch.
    let next = words(&format!("least(__local11+1,length(__local43)/{block}-1)"));
    let add_state = format!(
        "ARRAY[{}]",
        (1..=8)
            .map(|i| format!("(__local14[{i}]+__local25[{i}]) & {mask}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    let padded = format!(
        "hex(__local1) || '80' || repeat('0', CAST(({block} - (length(hex(__local1)) + 2 + {length}) % {block}) % {block} AS BIGINT)) || lpad(hex(CAST(octet_length(__local1) AS UHUGEINT)*8), {length}, '0')"
    );
    Some(format!(
        "(SELECT (WITH RECURSIVE __local10(__local11,__local12,__local13,__local14,__local15,__local43) AS (SELECT 0::bigint,0::bigint,{first},{initial},{initial},__local43 FROM (SELECT __local3 AS __local43 OFFSET 0) AS __local44 UNION ALL SELECT CASE WHEN __local12={rounds}-1 THEN __local11+1 ELSE __local11 END, CASE WHEN __local12={rounds}-1 THEN 0 ELSE __local12+1 END, CASE WHEN __local12={rounds}-1 THEN {next} WHEN __local12>=16 THEN list_append(__local13,__local21) ELSE __local13 END, CASE WHEN __local12={rounds}-1 THEN __local27 ELSE __local14 END, CASE WHEN __local12={rounds}-1 THEN __local27 ELSE __local25 END, __local43 FROM __local10 CROSS JOIN LATERAL (SELECT CASE WHEN __local12<16 THEN __local13[__local12+1] ELSE (__local13[__local12-15]+{small0}+__local13[__local12-6]+{small1}) & {mask} END AS __local21) AS __local20 CROSS JOIN LATERAL (SELECT (__local15[8]+{big1}+{ch}+{constants}[__local12+1]+__local21) & {mask} AS __local23, ({big0}+{maj}) & {mask} AS __local24) AS __local22 CROSS JOIN LATERAL (SELECT ARRAY[(__local23+__local24) & {mask},__local15[1],__local15[2],__local15[3],(__local15[4]+__local23) & {mask},__local15[5],__local15[6],__local15[7]] AS __local25) AS __local26 CROSS JOIN LATERAL (SELECT {add_state} AS __local27) AS __local28 WHERE __local11 < length(__local43)/{block}) SELECT from_hex(left(array_to_string(list_transform(__local14, lambda __local42: lpad(hex(__local42), {hexword}, '0')), ''), {output})) FROM __local10 WHERE __local11 = length(__local43)/{block}) FROM (SELECT {padded} AS __local3 FROM (SELECT {} AS __local1 OFFSET 0) AS __local0 OFFSET 0) AS __local2)",
        super::scalars::bytes(false)
    ))
}
