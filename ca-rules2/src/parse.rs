use crate::{Neighborhood, NeighborhoodType, ParseRuleError, Rule};
use std::{
    num::ParseIntError,
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

impl<const N: usize> CharPattern for [u8; N] {
    fn matches(&self, c: u8) -> bool {
        self.contains(&c)
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

impl<T> CharPattern for &T
where
    T: CharPattern,
{
    fn matches(&self, c: u8) -> bool {
        (*self).matches(c)
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

    /// Parse zero or more things separated by a given separator with a given
    /// parser function.
    fn parse_many_sep<T>(
        &mut self,
        sep: impl CharPattern,
        parser_fn: impl FnMut(&mut Self) -> Option<T>,
    ) -> Vec<T> {
        let mut result = Vec::new();
        let mut parser_fn = parser_fn;
        if let Some(item) = self.try_parse(&mut parser_fn) {
            result.push(item);

            let mut parser_fn = |parser: &mut Self| {
                parser.read_matches(&sep)?;
                parser_fn(parser)
            };

            while let Some(item) = self.try_parse(&mut parser_fn) {
                result.push(item);
            }
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
        while self.read_matches(&pattern).is_some() {
            len += 1;
        }
        &input[..len]
    }

    /// Try to read something that exactly matches the given byte string.
    fn read_matches_exact(&mut self, bytes: &[u8]) -> Option<()> {
        let input = self.input;
        if input.starts_with(bytes) {
            self.input = &input[bytes.len()..];
            Some(())
        } else {
            None
        }
    }

    /// Parse a single digit as a `u64`.
    fn parse_digit(&mut self) -> Option<u64> {
        let c = self.read_matches(b'0'..=b'9')?;
        Some((c - b'0') as u64)
    }

    /// Try to read zero or more digits and parse them as a `u64`.
    ///
    /// Returns `None` if there are no digits to parse.
    /// Returns `Some(Err(_))` if there are digits to parse but the number is
    /// too large to be represented by a `u64`.
    fn parse_number(&mut self) -> Option<Result<u64, ParseIntError>> {
        let digits = self.read_matches_many(b'0'..=b'9');
        (!digits.is_empty()).then(|| str::from_utf8(digits).unwrap().parse())
    }

    /// Parse a neighborhood type for a Life-like rule string.
    fn parse_neighborhood_type_life_like(&mut self) -> Option<NeighborhoodType> {
        match self.read() {
            Some(b'V' | b'v') => Some(NeighborhoodType::VonNeumann),
            Some(b'H' | b'h') => Some(NeighborhoodType::Hexagonal),
            None => Some(NeighborhoodType::Moore),
            _ => None,
        }
    }

    /// Parse a neighborhood type for a HROT rule string.
    fn parse_neighborhood_type_hrot(&mut self) -> Option<NeighborhoodType> {
        match self.read()? {
            b'M' | b'm' => Some(NeighborhoodType::Moore),
            b'N' | b'n' => Some(NeighborhoodType::VonNeumann),
            b'+' => Some(NeighborhoodType::Cross),
            b'H' | b'h' => Some(NeighborhoodType::Hexagonal),
            _ => None,
        }
    }

    /// Parse a single number or a range in the form `{min}-{max}`.
    ///
    /// If it is a single number, it is converted to a range with the same
    /// minimum and maximum.
    fn parse_range(&mut self) -> Option<Result<RangeInclusive<u64>, ParseIntError>> {
        let min = self.parse_number()?;
        let max = self.try_parse(|parser| {
            parser.read_matches(b'-')?;
            parser.parse_number()
        });
        match min {
            Err(err) => Some(Err(err)),
            Ok(min) => match max {
                Some(Err(err)) => Some(Err(err)),
                Some(Ok(max)) => Some(Ok(min..=max)),
                None => Some(Ok(min..=min)),
            },
        }
    }

    /// Parse a Life-like rule string with B/S notation or Catagolue notation.
    ///
    /// Returns `None` if this rule string is not using these notations.
    /// Returns `Some(Err(_))` if it is using these notation but there is some
    /// other error.
    fn parse_life_like_bs(&mut self) -> Option<Result<Rule, ParseRuleError>> {
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
        let neighborhood_type = self.parse_neighborhood_type_life_like()?;
        let neighborhood = Neighborhood::Totalistic(neighborhood_type, 1);

        // Check that there is no more input.
        if self.peek().is_some() {
            return None;
        }

        // Check that the birth and survival conditions are valid.
        let rule = Rule {
            states: 2,
            neighborhood,
            birth,
            survival,
        };
        if !rule.check_conditions() {
            return Some(Err(ParseRuleError::InvalidCondition));
        }

        Some(Ok(rule))
    }

    /// Parse a Life-like rule string with S/B notation.
    ///
    /// Returns `None` if this rule string is not using S/B notation.
    /// Returns `Some(Err(_))` if it is using S/B notation but there is some
    /// other error.
    fn parse_life_like_sb(&mut self) -> Option<Result<Rule, ParseRuleError>> {
        // Parse the survival sequence.
        let survival = self.parse_many(|parser| parser.parse_digit());

        // Parse the slash.
        self.read_matches(b'/')?;

        // Parse the birth sequence.
        let birth = self.parse_many(|parser| parser.parse_digit());

        // Parse the neighborhood type.
        let neighborhood_type = self.parse_neighborhood_type_life_like()?;
        let neighborhood = Neighborhood::Totalistic(neighborhood_type, 1);

        // Check that there is no more input.
        if self.peek().is_some() {
            return None;
        }

        // Check that the birth and survival conditions are valid.
        let rule = Rule {
            states: 2,
            neighborhood,
            birth,
            survival,
        };
        if !rule.check_conditions() {
            return Some(Err(ParseRuleError::InvalidCondition));
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
    fn parse_life_like(&mut self) -> Option<Result<Rule, ParseRuleError>> {
        self.try_parse(|parser| parser.parse_life_like_bs())
            .or_else(|| self.try_parse(|parser| parser.parse_life_like_sb()))
    }

    /// Parse a Generations rule string with B/S/C notation.
    ///
    /// Returns `None` if this rule string is not using B/S/C notation.
    /// Returns `Some(Err(_))` if it is using B/S/C notation but there is some
    /// other error.
    ///
    /// See [`parse_generations`] for more details.
    fn parse_generations_bsc(&mut self) -> Option<Result<Rule, ParseRuleError>> {
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
        let neighborhood_type = self.parse_neighborhood_type_life_like()?;
        let neighborhood = Neighborhood::Totalistic(neighborhood_type, 1);

        // Check that there is no more input.
        if self.peek().is_some() {
            return None;
        }

        // Check that the number of states is valid.
        if states.is_err() {
            return Some(Err(ParseRuleError::IntegerOverflow));
        }
        let states = states.unwrap();
        if states < 2 {
            return Some(Err(ParseRuleError::TooFewStates));
        }

        // Check that the birth and survival conditions are valid.
        let rule = Rule {
            states,
            neighborhood,
            birth,
            survival,
        };
        if !rule.check_conditions() {
            return Some(Err(ParseRuleError::InvalidCondition));
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
    fn parse_generations_sbc(&mut self) -> Option<Result<Rule, ParseRuleError>> {
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
        let neighborhood_type = self.parse_neighborhood_type_life_like()?;
        let neighborhood = Neighborhood::Totalistic(neighborhood_type, 1);

        // Check that there is no more input.
        if self.peek().is_some() {
            return None;
        }

        // Check that the number of states is valid.
        if states.is_err() {
            return Some(Err(ParseRuleError::IntegerOverflow));
        }
        let states = states.unwrap();
        if states < 2 {
            return Some(Err(ParseRuleError::TooFewStates));
        }

        // Check that the birth and survival conditions are valid.
        let rule = Rule {
            states,
            neighborhood,
            birth,
            survival,
        };
        if !rule.check_conditions() {
            return Some(Err(ParseRuleError::InvalidCondition));
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
    fn parse_generations_catagolue(&mut self) -> Option<Result<Rule, ParseRuleError>> {
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
        let neighborhood_type = self.parse_neighborhood_type_life_like()?;
        let neighborhood = Neighborhood::Totalistic(neighborhood_type, 1);

        // Check that there is no more input.
        if self.peek().is_some() {
            return None;
        }

        // Check that the number of states is valid.
        if states.is_err() {
            return Some(Err(ParseRuleError::IntegerOverflow));
        }
        let states = states.unwrap();
        if states < 2 {
            return Some(Err(ParseRuleError::TooFewStates));
        }

        // Check that the birth and survival conditions are valid.
        let rule = Rule {
            states,
            neighborhood,
            birth,
            survival,
        };
        if !rule.check_conditions() {
            return Some(Err(ParseRuleError::InvalidCondition));
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
    fn parse_generations(&mut self) -> Option<Result<Rule, ParseRuleError>> {
        self.try_parse(|parser| parser.parse_generations_bsc())
            .or_else(|| self.try_parse(|parser| parser.parse_generations_sbc()))
            .or_else(|| self.try_parse(|parser| parser.parse_generations_catagolue()))
    }

    /// Parse a HROT rule string with LtL notation.
    ///
    /// Returns `None` if this rule string is not using LtL notation.
    /// Returns `Some(Err(_))` if it is using LtL notation but there is some
    /// other error.
    ///
    /// See [`parse_hrot`] for more details.
    fn parse_hrot_ltl(&mut self) -> Option<Result<Rule, ParseRuleError>> {
        // Parse the radius.
        self.read_matches(b"Rr")?;
        let radius = self.parse_number()?;

        // Parse the comma.
        self.read_matches(b',')?;

        // Parse the number of states.
        self.read_matches(b"Cc")?;
        let states = self.parse_number()?;

        // Parse the comma.
        self.read_matches(b',')?;

        // Parse the center cell.
        self.read_matches(b"Mm")?;
        let center = self.read_matches(b"01")? - b'0';

        // Parse the comma.
        self.read_matches(b',')?;

        // Parse the survival sequence.
        self.read_matches(b"Ss")?;
        let smin = self.parse_number()?;
        self.read_matches_exact(b"..")?;
        let smax = self.parse_number()?;

        // Parse the comma.
        self.read_matches(b',')?;

        // Parse the birth sequence.
        self.read_matches(b"Bb")?;
        let bmin = self.parse_number()?;
        self.read_matches_exact(b"..")?;
        let bmax = self.parse_number()?;

        // Parse the comma.
        self.read_matches(b',')?;

        // Parse the neighborhood type.
        self.read_matches(b"Nn")?;
        let neighborhood_type = self.parse_neighborhood_type_hrot()?;

        // Check that there is no more input.
        if self.peek().is_some() {
            return None;
        }

        // Check that the radius is valid.
        if radius.is_err() {
            return Some(Err(ParseRuleError::IntegerOverflow));
        }
        let radius = radius.unwrap();
        if radius > i32::MAX as u64 {
            return Some(Err(ParseRuleError::IntegerOverflow));
        }

        let neighborhood = Neighborhood::Totalistic(neighborhood_type, radius as u32);

        // Check that the number of states is valid.
        if states.is_err() {
            return Some(Err(ParseRuleError::IntegerOverflow));
        }
        let states = states.unwrap().max(2);

        // Check that the birth and survival conditions are valid.
        if smin.is_err() || smax.is_err() || bmin.is_err() || bmax.is_err() {
            return Some(Err(ParseRuleError::IntegerOverflow));
        }
        let mut smin = smin.unwrap();
        let mut smax = smax.unwrap();
        let bmin = bmin.unwrap();
        let bmax = bmax.unwrap();

        if center == 1 {
            if smin == 0 || smax == 0 {
                return Some(Err(ParseRuleError::InvalidCondition));
            }
            smin -= 1;
            smax -= 1;
        }

        let survival = (smin..=smax).collect();
        let birth = (bmin..=bmax).collect();
        let rule = Rule {
            states,
            neighborhood,
            birth,
            survival,
        };
        if !rule.check_conditions() {
            return Some(Err(ParseRuleError::InvalidCondition));
        }

        Some(Ok(rule))
    }

    /// Parse a HROT rule string with Kellie Evans' notation.
    ///
    /// Returns `None` if this rule string is not using Kellie Evans' notation.
    /// Returns `Some(Err(_))` if it is using Kellie Evans' notation but there
    /// is some other error.
    ///
    /// See [`parse_hrot`] for more details.
    fn parse_hrot_ke(&mut self) -> Option<Result<Rule, ParseRuleError>> {
        // Parse the radius.
        let radius = self.parse_number()?;

        // Parse the comma.
        self.read_matches(b',')?;

        // Parse the birth sequence.
        let bmin = self.parse_number()?;
        self.read_matches(b',')?;
        let bmax = self.parse_number()?;

        // Parse the comma.
        self.read_matches(b',')?;

        // Parse the survival sequence.
        let smin = self.parse_number()?;
        self.read_matches(b',')?;
        let smax = self.parse_number()?;

        // Check that there is no more input.
        if self.peek().is_some() {
            return None;
        }

        // Check that the radius is valid.
        if radius.is_err() {
            return Some(Err(ParseRuleError::IntegerOverflow));
        }
        let radius = radius.unwrap();
        if radius > i32::MAX as u64 {
            return Some(Err(ParseRuleError::IntegerOverflow));
        }

        let neighborhood = Neighborhood::Totalistic(NeighborhoodType::Moore, radius as u32);

        // Check that the birth and survival conditions are valid.
        if smin.is_err() || smax.is_err() || bmin.is_err() || bmax.is_err() {
            return Some(Err(ParseRuleError::IntegerOverflow));
        }
        let mut smin = smin.unwrap();
        let mut smax = smax.unwrap();
        let bmin = bmin.unwrap();
        let bmax = bmax.unwrap();

        if smin == 0 || smax == 0 {
            return Some(Err(ParseRuleError::InvalidCondition));
        }
        smin -= 1;
        smax -= 1;

        let survival = (smin..=smax).collect();
        let birth = (bmin..=bmax).collect();
        let rule = Rule {
            states: 2,
            neighborhood,
            birth,
            survival,
        };
        if !rule.check_conditions() {
            return Some(Err(ParseRuleError::InvalidCondition));
        }

        Some(Ok(rule))
    }

    /// Parse a HROT rule string with HROT notation.
    ///
    /// Returns `None` if this rule string is not using HROT notation.
    /// Returns `Some(Err(_))` if it is using HROT notation but there is some
    /// other error.
    ///
    /// See [`parse_hrot`] for more details.
    fn parse_hrot_hrot(&mut self) -> Option<Result<Rule, ParseRuleError>> {
        // Parse the radius.
        self.read_matches(b"Rr")?;
        let radius = self.parse_number()?;

        // Parse the comma.
        self.read_matches(b',')?;

        // Parse the number of states.
        self.read_matches(b"Cc")?;
        let states = self.parse_number()?;

        // Parse the comma.
        self.read_matches(b',')?;

        // Parse the survival sequence.
        self.read_matches(b"Ss")?;
        let survival_list = self.parse_many_sep(b',', |parser| parser.parse_range());

        // Parse the comma.
        self.read_matches(b',')?;

        // Parse the birth sequence.
        self.read_matches(b"Bb")?;
        let birth_list = self.parse_many_sep(b',', |parser| parser.parse_range());

        // Parse the comma and the neighborhood type. This is optional.
        let neighborhood_type = if self.read_matches(b",").is_some() {
            self.read_matches(b"Nn")?;
            self.parse_neighborhood_type_hrot()?
        } else {
            NeighborhoodType::Moore
        };

        // Check that there is no more input.
        if self.peek().is_some() {
            return None;
        }

        // Check that the radius is valid.
        if radius.is_err() {
            return Some(Err(ParseRuleError::IntegerOverflow));
        }
        let radius = radius.unwrap();
        if radius > i32::MAX as u64 {
            return Some(Err(ParseRuleError::IntegerOverflow));
        }

        let neighborhood = Neighborhood::Totalistic(neighborhood_type, radius as u32);

        // Check that the number of states is valid.
        if states.is_err() {
            return Some(Err(ParseRuleError::IntegerOverflow));
        }
        let states = states.unwrap().max(2);

        // Check that the birth and survival conditions are valid.
        let mut survival = Vec::new();
        for range in survival_list {
            match range {
                Ok(range) => survival.extend(range),
                Err(_) => return Some(Err(ParseRuleError::IntegerOverflow)),
            }
        }

        let mut birth = Vec::new();
        for range in birth_list {
            match range {
                Ok(range) => birth.extend(range),
                Err(_) => return Some(Err(ParseRuleError::IntegerOverflow)),
            }
        }

        let rule = Rule {
            states,
            neighborhood,
            birth,
            survival,
        };
        if !rule.check_conditions() {
            return Some(Err(ParseRuleError::InvalidCondition));
        }

        Some(Ok(rule))
    }

    /// Parse a HROT rule string.
    ///
    /// Returns `None` if this is not a valid HROT rule string.
    /// Returns `Some(Err(_))` if it is a HROT rule string but there is some
    /// other error.
    ///
    /// See [`parse_hrot`] for more details.
    fn parse_hrot(&mut self) -> Option<Result<Rule, ParseRuleError>> {
        self.try_parse(|parser| parser.parse_hrot_ltl())
            .or_else(|| self.try_parse(|parser| parser.parse_hrot_ke()))
            .or_else(|| self.try_parse(|parser| parser.parse_hrot_hrot()))
    }

    /// Parse a rule string.
    ///
    /// This function supports the following kinds of rule strings:
    /// - Life-like rule, see [`parse_life_like`](Self::parse_life_like).
    /// - Generations rule, see [`parse_generations`](Self::parse_generations).
    /// - HROT rule, see [`parse_hrot`](Self::parse_hrot).
    fn parse_rule(&mut self) -> Option<Result<Rule, ParseRuleError>> {
        self.parse_life_like()
            .or_else(|| self.parse_generations())
            .or_else(|| self.parse_hrot())
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
/// The rule string is in the form `B{birth}/S{survival}`, where:
///
/// - `{birth}` is a sequence of digits. These are the numbers of neighbors
///   that cause a dead cell to become alive.
/// - `{survival}` is a sequence of digits. These are the numbers of neighbors
///   that cause a live cell to survive.
///
/// These sequences may be empty.
///
/// # S/B notation
///
/// The rule string is in the form `{survival}/{birth}`, where `{birth}` and
/// `{survival}` are the same as in the B/S notation.
///
/// # Catagolue notation
///
/// The rule string is in the form `b{birth}s{survival}`, where `{birth}` and
/// `{survival}` are the same as in the B/S notation.
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
pub fn parse_life_like(rule_string: &str) -> Result<Rule, ParseRuleError> {
    let mut parser = Parser::new(rule_string);

    parser
        .parse_life_like()
        .unwrap_or(Err(ParseRuleError::InvalidSyntax))
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
/// The rule string is in the form `B{birth}/S{survival}/{states}`, where:
///
/// - `{birth}` is a sequence of digits. These are the numbers of neighbors
///   that cause a dead cell to become alive. The sequence may be empty.
/// - `{survival}` is a sequence of digits. These are the numbers of neighbors
///   that cause a live cell to survive. The sequence may be empty.
/// - `{states}` is the number of states in the cellular automaton. It must be
///   greater than 1.
///
/// # S/B/C notation
///
/// The rule string is in the form `{survival}/{birth}/{states}`, where
/// `{birth}`, `{survival}`, and `{states}` are the same as in the B/S/C
/// notation.
///
/// # Catagolue notation
///
/// The rule string is in the form `g{states}b{birth}s{survival}`, where
/// `{birth}`, `{survival}`, and `{states}` are the same as in the B/S/C
/// notation.
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
pub fn parse_generations(rule_string: &str) -> Result<Rule, ParseRuleError> {
    let mut parser = Parser::new(rule_string);

    parser
        .parse_generations()
        .unwrap_or(Err(ParseRuleError::InvalidSyntax))
}

/// Parse a [higher-range outer-totalistic](https://conwaylife.com/wiki/Higher-range_outer-totalistic_cellular_automaton)
/// (also known as "HROT") rule string.
///
/// These rules are similar to Generations, but the radius of the neighborhood
/// may be greater than 1.
///
/// Three notations are supported: LtL (Large than Life) notation, Kellie Evans'
/// notation, and HROT notation.
///
/// The rule string is case-insensitive.
///
/// # LtL notation
///
/// The rule string is in the form `R{radius},C{states},M{center},S{smin}..{smax},B{bmin}..{bmax},N{neighborhood}`,
/// where:
///
/// - `{radius}` is the radius of the neighborhood. It must be greater than 0.
/// - `{states}` is the number of states in the cellular automaton. If it is
///   smaller than 2, it is treated as 2.
/// - `{center}` is either `0` or `1`. It indicates whether the center cell is
///   included when counting neighbors.
/// - `{smin}` and `{smax}` are the minimum and maximum number of neighbors
///   that cause a live cell to survive, respectively.
/// - `{bmin}` and `{bmax}` are the minimum and maximum number of neighbors
///   that cause a dead cell to become alive, respectively.
/// - `{neighborhood}` is the neighborhood type. Currently the parser only
///   supports the following neighborhood types:
///   - `M` for the Moore neighborhood.
///   - `N` for the von Neumann neighborhood.
///   - `+` for the cross neighborhood.
///   - `H` for the hexagonal neighborhood.
///
/// # Kellie Evans' notation
///
/// The rule string is in the form `{radius},{bmin},{bmax},{smin},{smax}`,
/// where `{radius}`, `{bmin}`, `{bmax}`, `{smin}`, and `{smax}` are the same as
/// in the LtL notation.
///
/// In this notation, the number of states is always 2, and the center cell is
/// always included when counting neighbors. The neighborhood type is always
/// Moore.
///
/// # HROT notation
///
/// The rule string is in the form `R{radius},C{states},S{slist},B{blist},N{neighborhood}`,
/// where:
///
/// - `{radius}` and `{states}` are the same as in the LtL notation.
/// - The center cell is always excluded when counting neighbors.
/// - `{slist}` and `{blist}` are lists of items separated by commas. Each item
///   is either a single number, or a range in the form `{min}-{max}`.
/// - `{neighborhood}` is the same as in the LtL notation, except that it may
///   be omitted. If it is omitted, the Moore neighborhood is assumed.
pub fn parse_hrot(rule_string: &str) -> Result<Rule, ParseRuleError> {
    let mut parser = Parser::new(rule_string);

    parser
        .parse_hrot()
        .unwrap_or(Err(ParseRuleError::InvalidSyntax))
}

/// Parse a rule string.
///
/// This function supports the following kinds of rule strings:
///
/// - Life-like rule, see [`parse_life_like`].
/// - Generations rule, see [`parse_generations`].
/// - HROT rule, see [`parse_hrot`].
///
/// See the documentation of each function for more details.
///
/// This function is also used in the [`FromStr`](std::str::FromStr) implementation
/// for [`Rule`](crate::Rule).
pub fn parse_rule(rule_string: &str) -> Result<Rule, ParseRuleError> {
    let mut parser = Parser::new(rule_string);

    parser
        .parse_rule()
        .unwrap_or(Err(ParseRuleError::InvalidSyntax))
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
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![3],
                survival: vec![2, 3],
            }
        );

        assert_eq!(
            parse_life_like("B2/S").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![2],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_life_like("B/S").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_life_like("B13/S012V").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::VonNeumann, 1),
                birth: vec![1, 3],
                survival: vec![0, 1, 2],
            }
        );

        assert_eq!(
            parse_life_like("B245/S3H").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Hexagonal, 1),
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
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![3],
                survival: vec![2, 3],
            }
        );

        assert_eq!(
            parse_life_like("2/").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![],
                survival: vec![2],
            }
        );

        assert_eq!(
            parse_life_like("/").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_life_like("012/13V").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::VonNeumann, 1),
                birth: vec![1, 3],
                survival: vec![0, 1, 2],
            }
        );

        assert_eq!(
            parse_life_like("3/245H").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Hexagonal, 1),
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
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![3],
                survival: vec![2, 3],
            }
        );

        assert_eq!(
            parse_life_like("b2s").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![2],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_life_like("bs").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_life_like("b13s012v").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::VonNeumann, 1),
                birth: vec![1, 3],
                survival: vec![0, 1, 2],
            }
        );

        assert_eq!(
            parse_life_like("b245s3h").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Hexagonal, 1),
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
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![3],
                survival: vec![2, 3],
            }
        );

        assert_eq!(
            parse_generations("B2/S/3").unwrap(),
            Rule {
                states: 3,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![2],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_generations("B/S/4").unwrap(),
            Rule {
                states: 4,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_generations("B13/S012/5V").unwrap(),
            Rule {
                states: 5,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::VonNeumann, 1),
                birth: vec![1, 3],
                survival: vec![0, 1, 2],
            }
        );

        assert_eq!(
            parse_generations("B245/S3/255H").unwrap(),
            Rule {
                states: 255,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Hexagonal, 1),
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
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![3],
                survival: vec![2, 3],
            }
        );

        assert_eq!(
            parse_generations("/2/3").unwrap(),
            Rule {
                states: 3,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![2],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_generations("//4").unwrap(),
            Rule {
                states: 4,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_generations("012/13/5V").unwrap(),
            Rule {
                states: 5,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::VonNeumann, 1),
                birth: vec![1, 3],
                survival: vec![0, 1, 2],
            }
        );

        assert_eq!(
            parse_generations("3/245/255H").unwrap(),
            Rule {
                states: 255,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Hexagonal, 1),
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
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![3],
                survival: vec![2, 3],
            }
        );

        assert_eq!(
            parse_generations("g3b2s").unwrap(),
            Rule {
                states: 3,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![2],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_generations("g4bs").unwrap(),
            Rule {
                states: 4,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![],
                survival: vec![],
            }
        );

        assert_eq!(
            parse_generations("g5b13s012v").unwrap(),
            Rule {
                states: 5,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::VonNeumann, 1),
                birth: vec![1, 3],
                survival: vec![0, 1, 2],
            }
        );

        assert_eq!(
            parse_generations("g255b245s3h").unwrap(),
            Rule {
                states: 255,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Hexagonal, 1),
                birth: vec![2, 4, 5],
                survival: vec![3],
            }
        );
    }

    #[test]
    fn test_parse_hrot_ltl() {
        assert_eq!(
            parse_hrot("R1,C0,M0,S2..3,B3..3,NM").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![3],
                survival: vec![2, 3],
            }
        );

        assert_eq!(
            parse_hrot("R5,C0,M1,S34..58,B34..45,NM").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 5),
                birth: vec![34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45],
                survival: vec![
                    33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52,
                    53, 54, 55, 56, 57
                ],
            }
        );

        assert_eq!(
            parse_hrot("R1,C0,M1,S1..1,B1..1,NN").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::VonNeumann, 1),
                birth: vec![1],
                survival: vec![0],
            }
        );

        assert_eq!(
            parse_hrot("R10,C255,M1,S2..3,B3..3,NM").unwrap(),
            Rule {
                states: 255,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 10),
                birth: vec![3],
                survival: vec![1, 2],
            }
        );

        assert_eq!(
            parse_hrot("R3,C2,M0,S2..2,B3..3,N+").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Cross, 3),
                birth: vec![3],
                survival: vec![2],
            }
        );
    }

    #[test]
    fn test_parse_hrot_ke() {
        assert_eq!(
            parse_hrot("1,3,3,3,4").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![3],
                survival: vec![2, 3],
            }
        );

        assert_eq!(
            parse_hrot("5,34,45,34,58").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 5),
                birth: vec![34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45],
                survival: vec![
                    33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52,
                    53, 54, 55, 56, 57
                ],
            }
        );

        assert_eq!(
            parse_hrot("1,1,1,1,1").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![1],
                survival: vec![0],
            }
        );
    }

    #[test]
    fn test_parse_hrot_hrot() {
        assert_eq!(
            parse_hrot("R1,C0,S2-3,B3").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
                birth: vec![3],
                survival: vec![2, 3],
            }
        );

        assert_eq!(
            parse_hrot("R5,C0,S33-57,B34-45").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 5),
                birth: vec![34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45],
                survival: vec![
                    33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52,
                    53, 54, 55, 56, 57
                ],
            }
        );

        assert_eq!(
            parse_hrot("R1,C0,S0,B1,NN").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::VonNeumann, 1),
                birth: vec![1],
                survival: vec![0],
            }
        );

        assert_eq!(
            parse_hrot("R10,C255,S1-2,B3,NM").unwrap(),
            Rule {
                states: 255,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 10),
                birth: vec![3],
                survival: vec![1, 2],
            }
        );

        assert_eq!(
            parse_hrot("R3,C2,S2,B3,N+").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Cross, 3),
                birth: vec![3],
                survival: vec![2],
            }
        );

        assert_eq!(
            parse_hrot("R3,C2,S6-10,12,B3,N+").unwrap(),
            Rule {
                states: 2,
                neighborhood: Neighborhood::Totalistic(NeighborhoodType::Cross, 3),
                birth: vec![3],
                survival: vec![6, 7, 8, 9, 10, 12],
            }
        );
    }
}
