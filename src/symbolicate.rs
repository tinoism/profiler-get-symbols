use crate::symbolicate_error::{Result as SymbolicateResult, SymbolicateError};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
extern crate serde;
extern crate serde_derive;
extern crate wasm_bindgen;

#[derive(Serialize)]
pub struct SymbolicateResponseJson {
    pub results: Vec<SymbolicateResponseResult>,
}

#[derive(Serialize)]
pub struct SymbolicateJob {
    pub memory_map: Vec<SymbolicateMemoryMap>,
    pub stacks: Vec<SymbolicateRequestStack>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SymbolicateMemoryMap {
    pub symbol_file_name: String,
    pub debug_id: String,
}

#[derive(Serialize)]
pub struct SymbolicateRequestStack {
    pub module_index_and_offset: Vec<u32>,
}

#[derive(Serialize, Clone)]
pub struct SymbolicateResponseResult {
    pub stacks: Vec<Vec<SymbolicateResponseStack>>,
    pub found_modules: SymbolicateFoundModule,
}

#[derive(Default, Serialize, Clone)]
pub struct SymbolicateResponseStack {
    pub module_offset: String,
    pub module: String,
    pub frame: u16,
    pub function: Option<String>,
    pub function_offset: Option<String>,
}

pub struct SymbolicateFunctionInfo {
    pub function: Option<String>,
    pub function_offset: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct SymbolicateFoundModule {
    symbolicate_found_status: HashMap<String, bool>,
}

impl SymbolicateResponseJson {
    pub fn as_json(&self) -> JsonValue {
        json!({
            "results": self.results.iter().map(|result| result.as_json()).collect::<Vec<_>>()
        })
    }

    pub fn new() -> Self {
        SymbolicateResponseJson {
            results: Vec::new(),
        }
    }
}

impl SymbolicateResponseResult {
    pub fn as_json(&self) -> JsonValue {
        json!({
            "stacks" : self.stacks.iter().map(|vec| vec.iter().map(|stack| stack.as_json()).collect::<Vec<_>>()).collect::<Vec<_>>(),
            "found_modules": self.found_modules.as_json()
        })
    }

    pub fn new(total_modules: usize) -> Self {
        // TODO
        let mut stacks = Vec::with_capacity(total_modules);
        for _ in 0..total_modules {
            // TODO: more elegant way to do this
            stacks.push(Default::default());
        }
        SymbolicateResponseResult {
            stacks: vec![stacks],
            found_modules: SymbolicateFoundModule::new(),
        }
    }

    pub fn add_stack(
        &mut self,
        stack: SymbolicateResponseStack,
        module_index: usize,
    ) -> SymbolicateResult<()> {
        match self.stacks.get(0) {
            Some(_) => match self.stacks[0].get(module_index) {
                Some(_) => {
                    self.stacks[0][module_index] = stack;
                    Ok(())
                }
                None => Err(SymbolicateError::ModuleIndexOutOfBound(
                    module_index,
                    module_index,
                )),
            },
            None => Err(SymbolicateError::ModuleIndexOutOfBound(
                module_index,
                module_index,
            )),
        }
    }
}

impl SymbolicateResponseStack {
    pub fn as_json(&self) -> JsonValue {
        serde_json::to_value(self).unwrap()
    }

    pub fn from(&mut self, function_info: SymbolicateFunctionInfo) {
        self.function = function_info.function;
        self.function_offset = function_info.function_offset;
    }

    pub fn from_memory_map(&mut self, memory_map: &SymbolicateMemoryMap, module_index: u16) {
        self.module = memory_map.symbol_file_name.clone();
        self.frame = module_index;
    }
}

impl SymbolicateJob {
    pub fn new() -> Self {
        SymbolicateJob {
            memory_map: Vec::new(),
            stacks: Vec::new(),
        }
    }

    pub fn get_number_modules(&self) -> SymbolicateResult<usize> {
        if self.memory_map.len() != self.stacks.len() {
            Err(SymbolicateError::InvalidInputError(
                "Unmatched length: memory_map and stacks",
            ))
        } else {
            Ok(self.memory_map.len())
        }
    }
}

impl SymbolicateMemoryMap {
    pub fn as_string(&self) -> String {
        format!("{}/{}", self.symbol_file_name, self.debug_id)
    }
}

impl SymbolicateFoundModule {
    pub fn new() -> Self {
        SymbolicateFoundModule {
            symbolicate_found_status: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: String, val: bool) {
        self.symbolicate_found_status.insert(key, val);
    }

    pub fn as_json(&self) -> JsonValue {
        serde_json::to_value(&self.symbolicate_found_status).unwrap()
    }

    pub fn from(&mut self, response_stack: &SymbolicateResponseStack, module_name_id: String) {
        self.symbolicate_found_status.insert(
            module_name_id,
            match response_stack.function {
                Some(_) => true,
                None => false,
            },
        );
    }
}

impl SymbolicateRequestStack {
    pub fn get_module_offset(&self) -> u32 {
        self.module_index_and_offset[1]
    }

    pub fn get_module_index(&self) -> u32 {
        self.module_index_and_offset[0]
    }
}
