// Unicode 14.0 general category ranges generated from the official UnicodeData.txt.
const RANGES: &[u8] = include_bytes!("unicode-14.bin");
pub(crate) fn word_category(ch: char) -> bool {
    let c = ch as u32;
    let (mut low, mut high) = (0, RANGES.len() / 8);
    while low < high {
        let mid = (low + high) / 2;
        let r = &RANGES[mid * 8..mid * 8 + 8];
        let a = u32::from_le_bytes(r[0..4].try_into().unwrap());
        let b = u32::from_le_bytes(r[4..8].try_into().unwrap());
        if c < a {
            high = mid;
        } else if c > b {
            low = mid + 1;
        } else {
            return true;
        }
    }
    false
}
