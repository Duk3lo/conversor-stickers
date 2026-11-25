pub const DEFAULT_EMOJI: &str = "🙂";
const MIN_EMOJIS: usize = 1;
const MAX_EMOJIS: usize = 3;
const MAX_BYTES_EMOJI: usize = 16;

const SELECTOR_EMOJI: char = '\u{FE0F}';
const UNION_ZWJ: char = '\u{200D}';
const KEY_FRAME: char = '\u{20E3}';

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EmojiError {
    #[error("cada sticker necesita de {MIN_EMOJIS} a {MAX_EMOJIS} emojis y se dieron {given}")]
    Count { given: usize },

    #[error(
        "«{text}» no es un emoji válido: debe ser un solo emoji de hasta {MAX_BYTES_EMOJI} bytes"
    )]
    NotValid { text: String },
}

fn is_base(char: char) -> bool {
    matches!(
        char as u32,
        0x00A9
            | 0x00AE
            | 0x203C
            | 0x2049
            | 0x2122
            | 0x2139
            | 0x2194..=0x21AA
            | 0x231A..=0x23FF
            | 0x24C2
            | 0x25AA..=0x25FE
            | 0x2600..=0x27BF
            | 0x2934
            | 0x2935
            | 0x2B05..=0x2B55
            | 0x3030
            | 0x303D
            | 0x3297
            | 0x3299
            | 0x1F004
            | 0x1F0CF
            | 0x1F170..=0x1F251
            | 0x1F300..=0x1F64F
            | 0x1F680..=0x1F6FF
            | 0x1F7E0..=0x1F7EB
            | 0x1F90C..=0x1FAFF
    )
}

fn is_flag(char: char) -> bool {
    ('\u{1F1E6}'..='\u{1F1FF}').contains(&char)
}

fn is_tone(char: char) -> bool {
    ('\u{1F3FB}'..='\u{1F3FF}').contains(&char)
}

fn validate_emoji(text: &str) -> Result<(), EmojiError> {
    if (1..=MAX_BYTES_EMOJI).contains(&text.len()) && is_sequence(text) {
        Ok(())
    } else {
        Err(EmojiError::NotValid {
            text: text.to_string(),
        })
    }
}

pub fn validate_emojis(emojis: &[String]) -> Result<(), EmojiError> {
    if !(MIN_EMOJIS..=MAX_EMOJIS).contains(&emojis.len()) {
        return Err(EmojiError::Count {
            given: emojis.len(),
        });
    }
    emojis.iter().try_for_each(|emoji| validate_emoji(emoji))
}

fn is_sequence(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let first = chars[0];
    if matches!(first, '#' | '*' | '0'..='9') {
        return matches!(chars[1..], [KEY_FRAME] | [SELECTOR_EMOJI, KEY_FRAME]);
    }
    if is_flag(first) {
        return chars.len() == 2 && is_flag(chars[1]);
    }
    chars.split(|&c| c == UNION_ZWJ).all(is_piece)
}

fn is_piece(piece: &[char]) -> bool {
    let [base, rest @ ..] = piece else {
        return false;
    };
    if !is_base(*base) {
        return false;
    }
    match rest {
        [] | [SELECTOR_EMOJI] => true,
        [SELECTOR_EMOJI, tone] | [tone] if is_tone(*tone) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn are_valid_sequences_and_simple_emojis() {
        for emoji in ["🙂", "👍🏽", "🏳️‍🌈", "❤️", "❤", "🇪🇸", "1️⃣", "👩‍💻"]
        {
            assert_eq!(validate_emoji(emoji), Ok(()), "{emoji}");
        }
    }

    #[test]
    fn not_are_valid_text_empty_nor_several_nor_too_long() {
        for text in [
            "a",
            "",
            "hola",
            "1",
            "🙂a",
            "🙂🙂",
            "🇪",
            "🇪🇸🇪🇸",
            "🙂\u{200D}",
            "\u{200D}🙂",
            "🙂\u{200D}a",
            "👨‍👩‍👧‍👦",
            "🙂 ",
        ] {
            assert!(validate_emoji(text).is_err(), "{text:?}");
        }
    }

    #[test]
    fn list_carries_of_1_to_3() {
        let one = vec!["🙂".to_string()];
        assert_eq!(validate_emojis(&one), Ok(()));
        let three = vec!["🙂".to_string(); 3];
        assert_eq!(validate_emojis(&three), Ok(()));
        let four = vec!["🙂".to_string(); 4];
        assert_eq!(validate_emojis(&four), Err(EmojiError::Count { given: 4 }));
        assert_eq!(validate_emojis(&[]), Err(EmojiError::Count { given: 0 }));
        let bad = vec!["🙂".to_string(), "x".to_string()];
        assert!(matches!(
            validate_emojis(&bad),
            Err(EmojiError::NotValid { .. })
        ));
    }
}
