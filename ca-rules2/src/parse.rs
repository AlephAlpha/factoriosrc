use crate::{NeighborhoodType, Rule, RuleStringError};
use std::{
    ops::{Range, RangeInclusive},
    str,
};

/// A pattern for matching a single character represented as a byte.
trait CharPattern {
    /// Returns `true` if the given character matches this pattern.
    fn matches(&self, c: u8) -> bool;
}

impl CharPattern for u8 {
    fn matches(&self, c: u8) -> bool {
        *self == c
    }
}

impl CharPattern for [u8] {
    fn matches(&self, c: u8) -> bool {
        self.contains(&c)
    }
}

impl<const N: usize> CharPattern for &[u8; N] {
    fn matches(&self, c: u8) -> bool {
        self.contains(&c)
    }
}

impl<F> CharPattern for F
where
    F: Fn(&u8) -> bool,
{
    fn matches(&self, c: u8) -> bool {
        self(&c)
    }
}

impl CharPattern for Range<u8> {
    fn matches(&self, c: u8) -> bool {
        self.contains(&c)
    }
}

impl CharPattern for RangeInclusive<u8> {
    fn matches(&self, c: u8) -> bool {
        self.contains(&c)
    }
}

/// A helper struct for parsing rule strings.
///
/// Inspired by the parser for [`IpAddr`](std::net::IpAddr) in Rust's standard
/// library.
struct Parser<'a> {
    input: &'a [u8],
}

impl<'a> Parser<'a> {
    /// Create a new parser from a string.
    const fn new(str: &'a str) -> Self {
        Self {
            input: str.as_bytes(),
        }
    }

    /// Try to parse something with a given parser function, and reset the
    /// parser if it fails.
    fn try_parse<T>(&mut self, parser_fn: impl FnOnce(&mut Self) -> Option<T>) -> Option<T> {
        let input = self.input;
        let result = parser_fn(self);
        if result.is_none() {
            self.input = input;
        }
        result
    }

    /// Parse zero or more things with a given parser function.
    fn parse_many<T>(&mut self, parser_fn: impl FnMut(&mut Self) -> Option<T>) -> Vec<T> {
        let mut result = Vec::new();
        let mut parser_fn = parser_fn;
        while let Some(item) = self.try_parse(&mut parser_fn) {
            result.push(item);
        }
        result
    }

    /// Peek at the next character without consuming it.
    fn peek(&self) -> Option<u8> {
        self.input.first().copied()
    }

    /// Read the next character and consume it.
    fn read(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.input = &self.input[1..];
        Some(c)
    }

    /// Try to read the next character and consume it if it matches the given
    /// pattern.
    fn read_matches(&mut self, pattern: impl CharPattern) -> Option<u8> {
        let c = self.peek()?;
        if pattern.matches(c) {
            self.input = &self.input[1..];
            Some(c)
        } else {
            None
        }
    }

    /// Try to read zero or more characters that match the given pattern.
    fn read_matches_many(&mut self, pattern: impl CharPattern) -> &'a [u8] {
        let input = self.input;
        let mut len = 0;
        while let Some(c) = self.peek() {
            if !pattern.matches(c) {
                break;
            }
            self.input = &self.input[1..];
            len += 1;
        }
        &input[..len]
    }

    /// Parse a single digit as a `u64`.
    fn parse_digit(&mut self) -> Option<u64> {
        let c = self.read_matches(b'0'..=b'9')?;
        Some((c - b'0') as u64)
    }

    /// Parse a number as a `u64`.
    fn parse_number(&mut self) -> Option<u64> {
        let digits = self.read_matches_many(b'0'..=b'9');
        str::from_utf8(digits).unwrap().parse().ok()
    }

    /// Parse a neighborhood type.
    fn parse_neighborhood_type(&mut self) -> Option<NeighborhoodType> {
        match self.read() {
            Some(b'V' | b'v') => Some(NeighborhoodType::VonNeumann),
            Some(b'H' | b'h') => Some(NeighborhoodType::Hexagonal),
            None => Some(NeighborhoodType::Moore),
            _ => None,
        }
    }

    /// Parse a Life-like rule string with B/S notation or Catagolue notation.
    ///
    /// Returns `None` if this rule string is not using these notations.
    /// Returns `Some(Err(_))` if it is using these notation but there is some
    /// other error.
    fn parse_life_like_bs(&mut self) -> Option<Result<Rule, RuleStringError>> {
        // Parse the birth sequence.
        self.read_matches(b"Bb")?;
        let birth = self.parse_many(|parser| parser.parse_digit());

        // Parse the slash. This is optional.
        // If there is no slash, this is a Catagolue rule string.
        self.read_matches(b'/');

        // Parse the survival sequence.
        self.read_matches(b"Ss")?;
        let survival = self.parse_many(|parser| parser.parse_digit());

        // Parse the neighborhood type.
        let neighborhood_type = self.parse_neighborhood_type()?;
        let neighbors = neighborhood_type.neighbors(1, true).unwrap();

        // Check that there is no more input.
        if self.peek().is_some() {
            return None;
        }

        // Check that the birth and survival conditions are valid.
        let rule = Rule {
            states: 2,
            neighbors,
            birth,
            survival,
        };
        if !rule.check_conditions() {
            return Some(Err(RuleStringError::InvalidCondition));
        }

        Some(Ok(rule))
    }

    /// Parse a Life-like rule string with S/B notation.
    ///
    /// Returns `None` if this rule string is not using S/B notation.
    /// Returns `Some(Err(_))` if it is using S/B notation but there is some
    /// other error.
    fn parse_life_like_sb(&mut self) -> Option<Result<Rule, RuleStringError>> {
        // Parse the survival sequence.
        let survival = self.parse_many(|parser| parser.parse_digit());

        // Parse the slash.
        self.read_matches(b'/')?;

        // Parse the birth sequence.
        let birth = self.parse_many(|parser| parser.parse_digit());

        // Parse the neighborhood type.
        let neighborhood_type = self.parse_neighborhood_type()?;
        let neighbors = neighborhood_type.neighbors(1, true).unwrap();

        // Check that there is no more input.
        if self.peek().is_some() {
            return None;
        }

        // Check that the birth and survival conditions are valid.
        let rule = Rule {
            states: 2,
            neighbors,
            birth,
            survival,
        };
        if !rule.check_conditions() {
            return Some(Err(RuleStringError::InvalidCondition));
        }

        Some(Ok(rule))
    }

    /// Parse a Life-like rule string.
    ///
    /// Returns `None` if this is not a valid Life-like rule string.
    /// Returns `Some(Err(_))` if it is a Life-like rule string but there is
    /// some other error.
    ///
    /// See [`parse_life_like`] for more details.
    fn parse_life_like(&mut self) -> Option<Result<Rule, RuleStringError>> {
        self.parse_life_like_bs()
            .or_else(|| self.parse_life_like_sb())
    }

    /// Parse a Generations rule string with B/S/C notation.
    ///
    /// Returns `None` if this rule string is not using B/S/C notation.
    /// Returns `Some(Err(_))` if it is using B/S/C notation but there is some
    /// other error.
    ///
    /// See [`parse_generations`] for more details.
    fn parse_generations_bsc(&mut self) -> Option<Result<Rule, RuleStringError>> {
        // Parse the birth sequence.
        self.read_matches(b"Bb")?;
        let birth = self.parse_many(|parser| parser.parse_digit());

        // Parse the slash.
        self.read_matches(b'/')?;

        // Parse the survival sequence.
        self.read_matches(b"Ss")?;
        let survival = self.parse_many(|parser| parser.parse_digit());

        // Parse the slash.
        self.read_matches(b'/')?;

        // Parse the number of states.
        let states = self.parse_number()?;

        // Parse the neighborhood type.
        let neighborhood_type = self.parse_neighborhood_type()?;
        let neighbors = neighborhood_type.neighbors(1, true).unwrap();

        // Check that there is no more input.
        if self.peek().is_some() {
            return None;
        }

        // Check that the number of states is valid.
        if states < 2 {
            return Some(Err(RuleStringError::InvalidNumberOfStates));
        }

        // Check that the birth and survival conditions are valid.
        let rule = Rule {
            states,
            neighbors,
            birth,
            survival,
        };
        if !rule.check_conditions() {
            return Some(Err(RuleStringError::InvalidCondition));
        }

        Some(Ok(rule))
    }

    /// Parse a Generations rule string with S/B/C notation.
    ///
    /// Returns `None` if this rule string is not using S/B/C notation.
    /// Returns `Some(Err(_))` if it is using S/B/C notation but there is some
    /// other error.
    ///
    /// See [`parse_generations`] for more details.
    fn parse_generations_sbc(&mut self) -> Option<Result<Rule, RuleStringError>> {
        // Parse the survival sequence.
        let survival = self.parse_many(|parser| parser.parse_digit());

        // Parse the slash.
        self.read_matches(b'/')?;

        // Parse the birth sequence.
        let birth = self.parse_many(|parser| parser.parse_digit());

        // Parse the slash.
        self.read_matches(b'/')?;

        // Parse the number of states.
        let states = self.parse_number()?;

        // Parse the neighborhood type.
        let neighborhood_type = self.parse_neighborhood_type()?;
        let neighbors = neighborhood_type.neighbors(1, true).unwrap();

        // Check that there is no more input.
        if self.peek().is_some() {
            return None;
        }

        // Check that the number of states is valid.
        if states < 2 {
            return Some(Err(RuleStringError::InvalidNumberOfStates));
        }

        // Check that the birth and survival conditions are valid.
        let rule = Rule {
            states,
            neighbors,
            birth,
            survival,
        };
        if !rule.check_conditions() {
            return Some(Err(RuleStringError::InvalidCondition));
        }

        Some(Ok(rule))
    }

    /// Parse a Generations rule string with Catagolue notation.
    ///
    /// Returns `None` if this rule string is not using Catagolue notation.
    /// Returns `Some(Err(_))` if it is using Catagolue notation but there is
    /// some other error.
    ///
    /// See [`parse_generations`] for more details.
    fn parse_generations_catagolue(&mut self) -> Option<Result<Rule, RuleStringError>> {
        // Parse the number of states.
        self.read_matches(b"gG")?;
        let states = self.parse_number()?;

        // Parse the birth sequence.
        self.read_matches(b"bB")?;
        let birth = self.parse_many(|parser| parser.parse_digit());

        // Parse the survival sequence.
        self.read_matches(b"sS")?;
        let survival = self.parse_many(|parser| parser.parse_digit());

        // Parse the neighborhood type.
        let neighborhood_type = self.parse_neighborhood_type()?;
        let neighbors = neighborhood_type.neighbors(1, true).unwrap();

        // Check that there is no more input.
        if self.peek().is_some() {
            return None;
        }

        // Check that the number of states is valid.
        if states < 2 {
            return Some(Err(RuleStringError::InvalidNumberOfStates));
        }

        // Check that the birth and survival conditions are valid.
        let rule = Rule {
            states,
            neighbors,
            birth,
            survival,
        };
        if !rule.check_conditions() {
            return Some(Err(RuleStringError::InvalidCondition));
        }

        Some(Ok(rule))
    }

    /// Parse a Generations rule string.
    ///
    /// Returns `None` if this is not a valid Generations rule string.
    /// Returns `Some(Err(_))` if it is a Generations rule string but there is
    /// some other error.
    ///
    /// See [`parse_generations`] for more details.
    fn parse_generations(&mut self) -> Option<Result<Rule, RuleStringError>> {
        self.parse_generations_bsc()
            .or_else(|| self.parse_generations_sbc())
            .or_else(|| self.parse_generations_catagolue())
    }
}

/// Parse a [Life-like](https://conwaylife.com/wiki/Life-like_cellular_automaton) rule string.
///
/// Three notations are supported: B/S/C notation, S/B/C notation, and the
/// notation used by Catagolue.
///
/// The rule string is case-insensitive.
///
/// # B/S notation
///
/// The rule string is in the form `B{birth}/S{survival}`, where `{birth}` and
/// `{survival}` are sequences of digits. The digits in `{birth}` are the
/// numbers of neighbors that cause a dead cell to become alive, and the digits
/// in `{survival}` are the numbers of neighbors that cause a live cell to
/// survive. These sequences may be empty.
///
/// # S/B notation
///
/// The rule string is in the form `{survival}/{birth}`, where `{birth}` and
/// `{survival}` are sequences of digits. The digits in `{birth}` are the
/// numbers of neighbors that cause a dead cell to become alive, and the digits
/// in `{survival}` are the numbers of neighbors that cause a live cell to
/// survive. These sequences may be empty.
///
/// # Catagolue notation
///
/// The rule string is in the form `b{birth}s{survival}`, where `{birth}` and
/// `{survival}` are sequences of digits. The digits in `{birth}` are the
/// numbers of neighbors that cause a dead cell to become alive, and the digits
/// in `{survival}` are the numbers of neighbors that cause a live cell to
/// survive. These sequences may be empty.
///
/// Since this parser is case-insensitive, the only difference between this
/// notation and the B/S notation is the lack of a slash.
///
/// This notation is used by [Catagolue](https://catagolue.hatsya.com/).
///
/// # Suffixes
///
/// The rule string may optionally have a suffix `V` or `H` to indicate the
/// neighborhood type. `V` means the von Neumann neighborhood, and `H` means
/// the hexagonal neighborhood. If there is no suffix, the Moore neighborhood is
/// assumed. All three neighborhood types have a radius of 1.
///
/// See [`NeighborhoodType`](crate::NeighborhoodType) for more information.
pub fn parse_life_like(rule_string: &str) -> Result<Rule, RuleStringError> {
    let mut parser = Parser::new(rule_string);

    parser
        .parse_life_like()
        .unwrap_or(Err(RuleStringError::InvalidSyntax))
}

/// Parse a [Generations](https://conwaylife.com/wiki/Generations) rule string.
///
/// Generations is similar to Life-like, but it may have more than two states.
///
/// Three notations are supported: B/S/C notation, S/B/C notation, and the
/// notation used by Catagolue.
///
/// The rule string is case-insensitive.
///
/// # B/S/C notation
///
/// The rule string is in the form `B{birth}/S{survival}/{states}`, where
/// `{birth}` and `{survival}` are sequences of digits, and `{states}` is a
/// integer greater than 1. The digits in `{birth}` are the numbers of
/// neighbors that cause a dead cell to become alive, and the digits in
/// `{survival}` are the numbers of neighbors that cause a live cell to
/// survive. These two sequences may be empty. `{states}` is the number of
/// states in the cellular automaton.
///
/// # S/B/C notation
///
/// The rule string is in the form `{survival}/{birth}/{states}`, where
/// `{birth}` and `{survival}` are sequences of digits, and `{states}` is a
/// integer greater than 1. The digits in `{birth}` are the numbers of
/// neighbors that cause a dead cell to become alive, and the digits in
/// `{survival}` are the numbers of neighbors that cause a live cell to
/// survive. These two sequences may be empty. `{states}` is the number of
/// states in the cellular automaton.
///
/// # Catagolue notation
///
/// The rule string is in the form `g{states}b{birth}s{survival}`, where
/// `{birth}` and `{survival}` are sequences of digits, and `{states}` is a
/// integer greater than 1. The digits in `{birth}` are the numbers of
/// neighbors that cause a dead cell to become alive, and the digits in
/// `{survival}` are the numbers of neighbors that cause a live cell to
/// survive. These two sequences may be empty. `{states}` is the number of
/// states in the cellular automaton.
///
/// This notation is used by [Catagolue](https://catagolue.hatsya.com/).
///
/// # Suffixes
///
/// The rule string may optionally have a suffix `V` or `H` to indicate the
/// neighborhood type. `V` means the von Neumann neighborhood, and `H` means
/// the hexagonal neighborhood. If there is no suffix, the Moore neighborhood is
/// assumed. All three neighborhood types have a radius of 1.
///
/// See [`NeighborhoodType`](crate::NeighborhoodType) for more information.
pub fn parse_generations(rule_string: &str) -> Result<Rule, RuleStringError> {
    let mut parser = Parser::new(rule_string);

    parser
        .parse_generations()
        .unwrap_or(Err(RuleStringError::InvalidSyntax))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_life_like_bs() {
        assert_eq!(
            parse_life_like("B3/S23").unwrap(),
            Rule {
                states: 2,
                neighbors: NeighborhoodType::Moore.neighbors(1, true).unwrap(),
                birth: vec![3],
                survival: vec![2, 3],
            }
        );

        assert_eq!(
            parse_life_like("B2/S").unwrap(),
            Rule {
                states: 2,
                neighbors: NeighborhoodType::Moore.neighbors(1, true).unwrap(),
                birth: vec![2],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_life_like("B/S").unwrap(),
            Rule {
                states: 2,
                neighbors: NeighborhoodType::Moore.neighbors(1, true).unwrap(),
                birth: vec![],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_life_like("B13/S012V").unwrap(),
            Rule {
                states: 2,
                neighbors: NeighborhoodType::VonNeumann.neighbors(1, true).unwrap(),
                birth: vec![1, 3],
                survival: vec![0, 1, 2],
            }
        );

        assert_eq!(
            parse_life_like("B245/S3H").unwrap(),
            Rule {
                states: 2,
                neighbors: NeighborhoodType::Hexagonal.neighbors(1, true).unwrap(),
                birth: vec![2, 4, 5],
                survival: vec![3],
            }
        );
    }

    #[test]
    fn test_parse_life_like_sb() {
        assert_eq!(
            parse_life_like("23/3").unwrap(),
            Rule {
                states: 2,
                neighbors: NeighborhoodType::Moore.neighbors(1, true).unwrap(),
                birth: vec![3],
                survival: vec![2, 3],
            }
        );

        assert_eq!(
            parse_life_like("2/").unwrap(),
            Rule {
                states: 2,
                neighbors: NeighborhoodType::Moore.neighbors(1, true).unwrap(),
                birth: vec![],
                survival: vec![2],
            }
        );

        assert_eq!(
            parse_life_like("/").unwrap(),
            Rule {
                states: 2,
                neighbors: NeighborhoodType::Moore.neighbors(1, true).unwrap(),
                birth: vec![],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_life_like("012/13V").unwrap(),
            Rule {
                states: 2,
                neighbors: NeighborhoodType::VonNeumann.neighbors(1, true).unwrap(),
                birth: vec![1, 3],
                survival: vec![0, 1, 2],
            }
        );

        assert_eq!(
            parse_life_like("3/245H").unwrap(),
            Rule {
                states: 2,
                neighbors: NeighborhoodType::Hexagonal.neighbors(1, true).unwrap(),
                birth: vec![2, 4, 5],
                survival: vec![3],
            }
        );
    }

    #[test]
    fn test_parse_life_like_catagolue() {
        assert_eq!(
            parse_life_like("b3s23").unwrap(),
            Rule {
                states: 2,
                neighbors: NeighborhoodType::Moore.neighbors(1, true).unwrap(),
                birth: vec![3],
                survival: vec![2, 3],
            }
        );

        assert_eq!(
            parse_life_like("b2s").unwrap(),
            Rule {
                states: 2,
                neighbors: NeighborhoodType::Moore.neighbors(1, true).unwrap(),
                birth: vec![2],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_life_like("bs").unwrap(),
            Rule {
                states: 2,
                neighbors: NeighborhoodType::Moore.neighbors(1, true).unwrap(),
                birth: vec![],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_life_like("b13s012v").unwrap(),
            Rule {
                states: 2,
                neighbors: NeighborhoodType::VonNeumann.neighbors(1, true).unwrap(),
                birth: vec![1, 3],
                survival: vec![0, 1, 2],
            }
        );

        assert_eq!(
            parse_life_like("b245s3h").unwrap(),
            Rule {
                states: 2,
                neighbors: NeighborhoodType::Hexagonal.neighbors(1, true).unwrap(),
                birth: vec![2, 4, 5],
                survival: vec![3],
            }
        );
    }

    #[test]
    fn test_parse_generations_bsc() {
        assert_eq!(
            parse_generations("B3/S23/2").unwrap(),
            Rule {
                states: 2,
                neighbors: NeighborhoodType::Moore.neighbors(1, true).unwrap(),
                birth: vec![3],
                survival: vec![2, 3],
            }
        );

        assert_eq!(
            parse_generations("B2/S/3").unwrap(),
            Rule {
                states: 3,
                neighbors: NeighborhoodType::Moore.neighbors(1, true).unwrap(),
                birth: vec![2],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_generations("B/S/4").unwrap(),
            Rule {
                states: 4,
                neighbors: NeighborhoodType::Moore.neighbors(1, true).unwrap(),
                birth: vec![],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_generations("B13/S012/5V").unwrap(),
            Rule {
                states: 5,
                neighbors: NeighborhoodType::VonNeumann.neighbors(1, true).unwrap(),
                birth: vec![1, 3],
                survival: vec![0, 1, 2],
            }
        );

        assert_eq!(
            parse_generations("B245/S3/255H").unwrap(),
            Rule {
                states: 255,
                neighbors: NeighborhoodType::Hexagonal.neighbors(1, true).unwrap(),
                birth: vec![2, 4, 5],
                survival: vec![3],
            }
        );
    }

    #[test]
    fn test_parse_generations_sbc() {
        assert_eq!(
            parse_generations("23/3/2").unwrap(),
            Rule {
                states: 2,
                neighbors: NeighborhoodType::Moore.neighbors(1, true).unwrap(),
                birth: vec![3],
                survival: vec![2, 3],
            }
        );

        assert_eq!(
            parse_generations("/2/3").unwrap(),
            Rule {
                states: 3,
                neighbors: NeighborhoodType::Moore.neighbors(1, true).unwrap(),
                birth: vec![2],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_generations("//4").unwrap(),
            Rule {
                states: 4,
                neighbors: NeighborhoodType::Moore.neighbors(1, true).unwrap(),
                birth: vec![],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_generations("012/13/5V").unwrap(),
            Rule {
                states: 5,
                neighbors: NeighborhoodType::VonNeumann.neighbors(1, true).unwrap(),
                birth: vec![1, 3],
                survival: vec![0, 1, 2],
            }
        );

        assert_eq!(
            parse_generations("3/245/255H").unwrap(),
            Rule {
                states: 255,
                neighbors: NeighborhoodType::Hexagonal.neighbors(1, true).unwrap(),
                birth: vec![2, 4, 5],
                survival: vec![3],
            }
        );
    }

    #[test]
    fn test_parse_generations_catagolue() {
        assert_eq!(
            parse_generations("g2b3s23").unwrap(),
            Rule {
                states: 2,
                neighbors: NeighborhoodType::Moore.neighbors(1, true).unwrap(),
                birth: vec![3],
                survival: vec![2, 3],
            }
        );

        assert_eq!(
            parse_generations("g3b2s").unwrap(),
            Rule {
                states: 3,
                neighbors: NeighborhoodType::Moore.neighbors(1, true).unwrap(),
                birth: vec![2],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_generations("g4bs").unwrap(),
            Rule {
                states: 4,
                neighbors: NeighborhoodType::Moore.neighbors(1, true).unwrap(),
                birth: vec![],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_generations("g5b13s012v").unwrap(),
            Rule {
                states: 5,
                neighbors: NeighborhoodType::VonNeumann.neighbors(1, true).unwrap(),
                birth: vec![1, 3],
                survival: vec![0, 1, 2],
            }
        );

        assert_eq!(
            parse_generations("g255b245s3h").unwrap(),
            Rule {
                states: 255,
                neighbors: NeighborhoodType::Hexagonal.neighbors(1, true).unwrap(),
                birth: vec![2, 4, 5],
                survival: vec![3],
            }
        );
    }
}
