use serde::Serialize;
use crate::error::GetSymbolsError;
use serde_json::Error as SerdeJsonError;
use std::fmt::{self};
use wasm_bindgen::JsValue;

pub type Result<T> = std::result::Result<T, SymbolicateError>;

#[derive(Debug)]
pub enum SymbolicateError {
    InvalidInputError(&'static str),
    UnmatchedModuleIndex(usize, usize),
    ModuleIndexOutOfBound(usize, usize),
    CompactSymbolTableError(GetSymbolsError),
    JsonParseArrayError,
    CallbackError,
    SerdeError(SerdeJsonError),
    JsValueError(JsValue),
}

impl fmt::Display for SymbolicateError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            SymbolicateError::InvalidInputError(ref invalid_input) => {
                write!(f, "Invalid input: {}", invalid_input)
            }
            SymbolicateError::UnmatchedModuleIndex(ref expected, ref actual) => write!(
                f,
                "Unmatched module index: Expected {}, but received {}",
                expected, actual
            ),
            SymbolicateError::ModuleIndexOutOfBound(total_len, module_index) => write! (
                f,
                "ModuleIndexOutOfBound: Total length of array is {}, but received {} as module index", total_len, module_index
            ),
            SymbolicateError::CompactSymbolTableError(ref get_symbol_error) => {
                write!(f, "GetSymbolsError error: {:?}", get_symbol_error.to_string())
            },
            SymbolicateError::JsonParseArrayError => {
                write!(f, "JsonParseArrayError")
            },
            SymbolicateError::SerdeError(ref serde_json_error) => {
                write!(f, "SerdeError: {}", serde_json_error.to_string())
            },
            SymbolicateError::JsValueError(ref js_value) => {
                write!(f, "{:?}",  js_value)
            },
            SymbolicateError::CallbackError => {
                write!(f, "CallbackError: ")
            }
        }
    }
}

impl From<SerdeJsonError> for SymbolicateError {
    fn from(err: SerdeJsonError) -> SymbolicateError {
        SymbolicateError::SerdeError(err)
    }
}

impl From<JsValue> for SymbolicateError {
    fn from(err: JsValue) -> SymbolicateError {
        SymbolicateError::JsValueError(err)
    }
}

impl SymbolicateError {
    pub fn enum_as_string(&self) -> &'static str {
        match *self {
            SymbolicateError::InvalidInputError(_) => "InvalidInputError",
            SymbolicateError::UnmatchedModuleIndex(_, _) => "UnmatchedModuleIndex",
            SymbolicateError::ModuleIndexOutOfBound(_, _) => "ModuleIndexOutOfBound",
            SymbolicateError::CompactSymbolTableError(_) => "CompactSymbolTableError",
            SymbolicateError::JsonParseArrayError => "JsonParseArrayError",
            SymbolicateError::SerdeError(_) => "SerdeError",
            SymbolicateError::JsValueError(_) => "JsValueError",
            SymbolicateError::CallbackError => "CallbackError",
        }
    }
}

#[derive(Serialize)]
pub struct SymbolicateErrorJson {
    error_type: String,
    error_msg: String,
}

impl SymbolicateErrorJson {
    pub fn from_error(err: SymbolicateError) -> Self {
        SymbolicateErrorJson {
            error_type: err.enum_as_string().to_string(),
            error_msg: err.to_string(),
        }
    }
}
