use std::borrow::Cow;

use crate::{CreateParserState, ParseStatus, Parser};

/// State of a repeat parser.
#[derive(Debug, PartialEq, Eq)]
pub struct RepeatParserState<P: Parser> {
    pub(crate) new_state_in_progress: bool,
    pub(crate) last_state: P::PartialState,
    pub(crate) outputs: Vec<P::Output>,
}

impl<P: Parser> Clone for RepeatParserState<P>
where
    P::PartialState: Clone,
    P::Output: Clone,
{
    fn clone(&self) -> Self {
        Self {
            new_state_in_progress: self.new_state_in_progress,
            last_state: self.last_state.clone(),
            outputs: self.outputs.clone(),
        }
    }
}

impl<P: Parser> RepeatParserState<P> {
    /// Create a new repeat parser state.
    pub fn new(state: P::PartialState, outputs: Vec<P::Output>) -> Self {
        Self {
            new_state_in_progress: false,
            last_state: state,
            outputs,
        }
    }
}

impl<P: Parser> Default for RepeatParserState<P>
where
    P::PartialState: Default,
{
    fn default() -> Self {
        RepeatParserState {
            new_state_in_progress: false,
            last_state: Default::default(),
            outputs: Default::default(),
        }
    }
}

/// A parser for a repeat of two parsers.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct RepeatParser<P> {
    pub(crate) parser: P,
    length_range: std::ops::RangeInclusive<usize>,
}

impl<P> Default for RepeatParser<P>
where
    P: Default,
{
    fn default() -> Self {
        RepeatParser {
            parser: Default::default(),
            length_range: 0..=usize::MAX,
        }
    }
}

impl<P> RepeatParser<P> {
    /// Create a new repeat parser.
    pub fn new(parser: P, length_range: std::ops::RangeInclusive<usize>) -> Self {
        Self {
            parser,
            length_range,
        }
    }
}

impl<O, PA, P: Parser<Output = O, PartialState = PA> + CreateParserState> CreateParserState
    for RepeatParser<P>
where
    P::PartialState: Clone,
    P::Output: Clone,
{
    fn create_parser_state(&self) -> <Self as Parser>::PartialState {
        RepeatParserState {
            new_state_in_progress: false,
            last_state: self.parser.create_parser_state(),
            outputs: Vec::new(),
        }
    }
}

impl<O, PA, P: Parser<Output = O, PartialState = PA> + CreateParserState> Parser for RepeatParser<P>
where
    P::PartialState: Clone,
    P::Output: Clone,
{
    type Output = Vec<O>;
    type PartialState = RepeatParserState<P>;

    fn parse<'a>(
        &self,
        state: &Self::PartialState,
        input: &'a [u8],
    ) -> crate::ParseResult<ParseStatus<'a, Self::PartialState, Self::Output>> {
        let mut state = state.clone();
        let mut remaining = input;
        loop {
            let result = self.parser.parse(&state.last_state, remaining);
            match result {
                Ok(ParseStatus::Finished {
                    result,
                    remaining: new_remaining,
                }) => {
                    state.outputs.push(result);
                    state.last_state = self.parser.create_parser_state();
                    state.new_state_in_progress = false;
                    remaining = new_remaining;
                    // If this is the maximum number of times we are repeating the parser,
                    // return the finished state immediately
                    if self.length_range.end() == &state.outputs.len() {
                        return Ok(ParseStatus::Finished {
                            result: state.outputs,
                            remaining,
                        });
                    }
                    // Otherwise, if we are out of input, return an empty required next state
                    if remaining.is_empty() {
                        // If this is a valid place for the sequence to stop, there is no required next state
                        // parsing an invalid sequence would be valid to stop the sequence
                        let mut required_next = Cow::default();
                        // Otherwise, the sequence must continue with another item
                        // Grab the required next state from that item
                        if !self.length_range.contains(&state.outputs.len()) {
                            if let Ok(ParseStatus::Incomplete {
                                required_next: new_required_next,
                                ..
                            }) = self.parser.parse(&state.last_state, remaining)
                            {
                                required_next = new_required_next;
                            }
                        }

                        return Ok(ParseStatus::Incomplete {
                            new_state: state,
                            required_next,
                        });
                    }
                }
                // If the parser is incomplete, we are out of input and we need to return
                Ok(ParseStatus::Incomplete {
                    new_state,
                    required_next,
                }) => {
                    state.last_state = new_state;
                    state.new_state_in_progress = true;
                    return Ok(ParseStatus::Incomplete {
                        new_state: state,
                        required_next,
                    });
                }
                // If we fail to parse, try to end the sequence
                // We can only end the sequence if the current state is not in progress
                // and this is in the valid range of times to repeat
                Err(e) => {
                    if !state.new_state_in_progress
                        && self.length_range.contains(&state.outputs.len())
                    {
                        return Ok(ParseStatus::Finished {
                            result: state.outputs,
                            remaining,
                        });
                    } else {
                        return Err(e);
                    }
                }
            }
        }
    }
}

#[test]
fn repeat_parser() {
    use crate::{IntegerParser, LiteralParser, ParserExt};
    let parser = RepeatParser::new(LiteralParser::from("a"), 1..=3);
    let state = parser.create_parser_state();
    let result = parser.parse(&state, b"aaa");
    assert_eq!(
        result,
        Ok(ParseStatus::Finished {
            result: vec![(); 3],
            remaining: b"",
        })
    );

    let int_parser = IntegerParser::new(1..=3);
    let parser = RepeatParser::new(int_parser.clone(), 1..=3);
    let state = parser.create_parser_state();
    let result = parser.parse(&state, b"123");
    assert_eq!(
        result,
        Ok(ParseStatus::Finished {
            result: vec![1, 2, 3],
            remaining: b"",
        })
    );

    let parser = RepeatParser::new(int_parser.clone(), 1..=3);
    let state = parser.create_parser_state();
    let result = parser.parse(&state, b"12");
    assert_eq!(
        result,
        Ok(ParseStatus::Incomplete {
            new_state: RepeatParserState {
                new_state_in_progress: false,
                last_state: int_parser.create_parser_state(),
                outputs: vec![1, 2],
            },
            required_next: Default::default()
        })
    );

    // It is not valid to stop the sequence here, required next must be some
    let separated_int_parser = LiteralParser::new("  ").ignore_output_then(int_parser);
    let repeat_separated_int_parser = RepeatParser::new(separated_int_parser.clone(), 3..=5);
    let state = repeat_separated_int_parser.create_parser_state();
    let result = repeat_separated_int_parser.parse(&state, b"  1  2");
    assert_eq!(
        result,
        Ok(ParseStatus::Incomplete {
            new_state: RepeatParserState {
                new_state_in_progress: false,
                last_state: separated_int_parser.create_parser_state(),
                outputs: vec![1, 2],
            },
            required_next: "  ".into()
        })
    );

    // It is valid to stop here. Required next must be none
    let state = repeat_separated_int_parser.create_parser_state();
    let result = repeat_separated_int_parser.parse(&state, b"  1  2  3");
    assert_eq!(
        result,
        Ok(ParseStatus::Incomplete {
            new_state: RepeatParserState {
                new_state_in_progress: false,
                last_state: separated_int_parser.create_parser_state(),
                outputs: vec![1, 2, 3],
            },
            required_next: Default::default()
        })
    );
}
