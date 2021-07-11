use std::{iter::Peekable, str::Chars};

use thiserror::Error;

#[derive(Debug)]
pub struct Query {
    parameters: Vec<Parameter>,
}

impl Query {
    pub fn get_first_value<S: AsRef<str>>(&self, search: S) -> Option<String> {
        for param in &self.parameters {
            match param {
                Parameter::Value(key, value) if key == search.as_ref() => {
                    return Some(value.clone())
                }
                _ => continue,
            }
        }

        None
    }

    pub fn bool_present<S: AsRef<str>>(&self, search: S) -> bool {
        for param in &self.parameters {
            match param {
                Parameter::Boolean(key) if key == search.as_ref() => return true,
                _ => continue,
            }
        }

        false
    }

    fn uncode_string<S: AsRef<str>>(urlencoded: S) -> Result<String, QueryParseError> {
        let mut uncoded: Vec<u8> = vec![];

        let mut chars = urlencoded.as_ref().chars();
        loop {
            match chars.next() {
                Some('%') => match (chars.next(), chars.next()) {
                    (Some(upper), Some(lower)) => uncoded.push(Self::from_hex(upper, lower)?),
                    (Some(upper), None) => {
                        return Err(QueryParseError::IncompletePercent(format!("%{}", upper)))
                    }
                    _ => return Err(QueryParseError::IncompletePercent("%".into())),
                },
                Some(c) => {
                    let mut utf8 = vec![0; c.len_utf8()];
                    c.encode_utf8(&mut utf8);
                    uncoded.extend_from_slice(&utf8);
                }
                None => {
                    return Ok(String::from_utf8(uncoded).map_err(|_| QueryParseError::InvalidUtf8)?)
                }
            }
        }
    }

    fn from_hex(upper: char, lower: char) -> Result<u8, QueryParseError> {
        let digit = |c: char| -> Result<u8, QueryParseError> {
            c.to_digit(16)
                .map(|big| big as u8)
                .ok_or(QueryParseError::ImproperHex(upper))
        };

        Ok((digit(upper)? * 16) + digit(lower)?)
    }
}

impl std::str::FromStr for Query {
    type Err = QueryParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parameters: Vec<Parameter> = vec![];
        let splits = s.split('&');

        for split in splits {
            let splits: Vec<&str> = split.splitn(2, '=').collect();

            match splits.len() {
                1 => parameters.push(Parameter::Boolean(splits[0].into())),
                2 => parameters.push(Parameter::Value(
                    splits[0].into(),
                    Self::uncode_string(splits[1])?,
                )),
                _ => unreachable!(),
            }
        }

        Ok(Self { parameters })
    }
}

#[derive(Debug)]
pub enum Parameter {
    Boolean(String),
    Value(String, String),
}

#[derive(Error, Debug, PartialEq)]
pub enum QueryParseError {
    #[error("'{0}' was in a url encoded character but is not valid hex")]
    ImproperHex(char),
    #[error("'{0}' is not valid percent encoding")]
    IncompletePercent(String),
    #[error("the query did not resolve to valid utf8")]
    InvalidUtf8,
}
