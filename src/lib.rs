#[macro_use]
extern crate serde_json;
extern crate futures;
extern crate goblin;
extern crate js_sys;
extern crate object;
extern crate pdb as pdb_crate;
extern crate scroll;
extern crate serde;
extern crate uuid;
extern crate wasm_bindgen;
extern crate wasm_bindgen_futures;

// #[macro_use]
// extern crate serde_derive;
// #![feature(proc_macro, generators)]
// extern crate futures_await as futures;
// use futures::prelude::*;
mod compact_symbol_table;
mod elf;
mod error;
mod macho;
mod pdb;
mod symbolicate;
mod symbolicate_error; // TODO
                       // #![feature(async_await)]
use crate::error::{GetSymbolsError, GetSymbolsErrorJson, Result};
use crate::symbolicate::*;
use crate::symbolicate_error::{
    Result as SymbolicateResult, SymbolicateError, SymbolicateErrorJson,
};
use futures::{future, Future};
use goblin::{mach, Hint};
use serde_json::Value as JsonValue;
use std::io::Cursor;
use std::mem;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{future_to_promise, JsFuture};
// use futures::prelude::*;
// use futures::future::{self, FutureResult};

// use wasm_bindgen_futures::futures_0_3::JsFuture;

#[wasm_bindgen]
pub struct CompactSymbolTable {
    addr: Vec<u32>,
    index: Vec<u32>,
    buffer: Vec<u8>,
}

#[wasm_bindgen]
impl CompactSymbolTable {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            addr: vec![],
            index: vec![],
            buffer: vec![],
        }
    }

    pub fn take_addr(&mut self) -> Vec<u32> {
        mem::replace(&mut self.addr, vec![])
    }
    pub fn take_index(&mut self) -> Vec<u32> {
        mem::replace(&mut self.index, vec![])
    }
    pub fn take_buffer(&mut self) -> Vec<u8> {
        mem::replace(&mut self.buffer, vec![])
    }
}

/// WasmMemBuffer lets you allocate a chunk of memory on the wasm heap and
/// directly initialize it from JS without a copy. The constructor takes the
/// allocation size and a callback function which does the initialization.
/// This is useful if you need to get very large amounts of data from JS into
/// wasm (for example, the contents of a 1.7GB libxul.so).
#[wasm_bindgen]
pub struct WasmMemBuffer {
    buffer: Vec<u8>,
}

#[wasm_bindgen]
impl WasmMemBuffer {
    /// Create the buffer and initialize it synchronously in the callback function.
    /// f is called with one argument: the Uint8Array that wraps our buffer.
    /// f should not return anything; its return value is ignored.
    /// f must not call any exported wasm functions! Anything that causes the
    /// wasm heap to resize will invalidate the typed array's internal buffer!
    /// Do not hold on to the array that is passed to f after f completes.
    #[wasm_bindgen(constructor)]
    pub fn new(byte_length: u32, f: &js_sys::Function) -> Self {
        // See https://github.com/rustwasm/wasm-bindgen/issues/1643 for how
        // to improve this method.
        let mut buffer = vec![0; byte_length as usize];
        unsafe {
            // Let JavaScript fill the buffer without making a copy.
            // We give the callback function access to the wasm memory via a
            // JS Uint8Array which wraps the underlying wasm memory buffer at
            // the appropriate offset and length.
            // The callback function is supposed to mutate the contents of
            // buffer. However, the "&mut" here is a bit of a lie:
            // Uint8Array::view takes an immutable reference to a slice, not a
            // mutable one. This is rather sketchy but seems to work for now.
            // https://github.com/rustwasm/wasm-bindgen/issues/1079#issuecomment-508577627
            let array = js_sys::Uint8Array::view(&mut buffer);
            f.call1(&JsValue::NULL, &JsValue::from(array))
                .expect("The callback function should not throw");
        }
        Self { buffer }
    }
}

fn get_compact_symbol_table_impl(
    binary_data: &[u8],
    debug_data: &[u8],
    breakpad_id: &str,
) -> Result<compact_symbol_table::CompactSymbolTable> {
    let mut reader = Cursor::new(binary_data);
    match goblin::peek(&mut reader)? {
        Hint::Elf(_) => elf::get_compact_symbol_table(binary_data, breakpad_id),
        Hint::Mach(_) => macho::get_compact_symbol_table(binary_data, breakpad_id),
        Hint::MachFat(_) => {
            let mut first_error = None;
            let multi_arch = mach::MultiArch::new(binary_data)?;
            for fat_arch in multi_arch.iter_arches().filter_map(std::result::Result::ok) {
                let arch_slice = fat_arch.slice(binary_data);
                match macho::get_compact_symbol_table(arch_slice, breakpad_id) {
                    Ok(table) => return Ok(table),
                    Err(err) => first_error = Some(err),
                }
            }
            Err(first_error.unwrap_or_else(|| {
                GetSymbolsError::InvalidInputError("Incompatible system architecture")
            }))
        }
        Hint::PE => pdb::get_compact_symbol_table(debug_data, breakpad_id),
        _ => Err(GetSymbolsError::InvalidInputError(
            "goblin::peek fails to read",
        )),
    }
}

#[wasm_bindgen]
pub fn get_compact_symbol_table(
    binary_data: &WasmMemBuffer,
    debug_data: &WasmMemBuffer,
    breakpad_id: &str,
) -> std::result::Result<CompactSymbolTable, JsValue> {
    match get_compact_symbol_table_impl(&binary_data.buffer, &debug_data.buffer, breakpad_id) {
        Result::Ok(table) => Ok(CompactSymbolTable {
            addr: table.addr,
            index: table.index,
            buffer: table.buffer,
        }),
        Result::Err(err) => {
            let error_type = GetSymbolsErrorJson::from_error(err);
            Err(JsValue::from_serde(&error_type).unwrap())
        }
    }
}

pub fn get_compact_symbol_table_internal(
    binary_data: &WasmMemBuffer,
    debug_data: &WasmMemBuffer,
    breakpad_id: &str,
) -> Result<CompactSymbolTable> {
    match get_compact_symbol_table_impl(&binary_data.buffer, &debug_data.buffer, breakpad_id) {
        Result::Ok(table) => Ok(CompactSymbolTable {
            addr: table.addr,
            index: table.index,
            buffer: table.buffer,
        }),
        Err(err) => Err(err) 
    }
}

/// wasm ffi for javascript to get symbolicate json response
#[wasm_bindgen]
pub fn get_symbolicate_response(
    data: &JsValue,
    read_buffer_callback: &js_sys::Function,    // read_buffer_callback takes two param: fileName, debugId
) -> std::result::Result<JsValue, JsValue> {
    match process_symbolicate_request(data, read_buffer_callback) {
        SymbolicateResult::Ok(resp) => Ok(resp),
        SymbolicateResult::Err(err) => {
            let error_type = SymbolicateErrorJson::from_error(err);
            Err(JsValue::from_serde(&error_type).unwrap())
        }
    }
}

#[wasm_bindgen]
extern "C" {
    fn printFromRust(s: String);
    pub type GetSymbolTableParamWrapper;

    #[wasm_bindgen(structural, method)]
    fn getInnerDebugData(this: &GetSymbolTableParamWrapper) -> WasmMemBuffer;
    #[wasm_bindgen(structural, method)]
    fn getInnerBinaryData(this: &GetSymbolTableParamWrapper) -> WasmMemBuffer;
    #[wasm_bindgen(structural, method)]
    fn getInnerDebugID(this: &GetSymbolTableParamWrapper) -> String;
}

/// Given a JS request JSON, parse and return a promise containing
/// the symbolication response or the error
/// Sometimes "jobs" wraps sometimes not, this function will handle both scenarios
/// @data: symbolicate request json
/// @read_buffer_callback: javascript callback to read the file name from memory
fn process_symbolicate_request(
    data: &JsValue,
    read_buffer_callback: &js_sys::Function,
) -> SymbolicateResult<JsValue> {
    let data_de: JsonValue = data.into_serde().unwrap();
    let symbolicate_jobs: Vec<SymbolicateJob> = match data_de.get("jobs") {
        // If the json starts with a 'jobs' key (aka, with a lot of jobs)
        // deconstruct into the array by first
        Some(jobs) => jobs
            .as_array()
            .ok_or_else(|| SymbolicateError::JsonParseArrayError)?
            .iter()
            .map(|x| parse_job(x).or_else(|e| Err(e)).unwrap())
            .collect::<Vec<_>>(),
        // else, parse the job object directly
        None => vec![parse_job(&data_de).unwrap()],
    };

    // place holder for storing the response from multiple jobs
    let mut return_val = JsValue::NULL;

    for (_, symbolicate_job) in symbolicate_jobs.iter().enumerate() {
        // Instantiate the handler with  get_number_modules()
        // Right now the handler will be responsible PER JOB (PER RESULT),
        // Later we can consider having a larger wrapper to wrap the JobHandler class
        let mut json_response_assembler =
            SymbolicateJsonAssembler::new(symbolicate_job.get_number_modules().unwrap()); // change to ?
        for module_index in 0..symbolicate_job.get_number_modules().unwrap() {
            // change to ?
            if module_index != symbolicate_job.stacks[module_index].get_module_index() as usize {
                return Err(SymbolicateError::UnmatchedModuleIndex(
                    module_index,
                    symbolicate_job.stacks[module_index].get_module_offset() as usize,
                ));
            }
            json_response_assembler.process_job_and_add_to_futures(
                symbolicate_job,
                module_index as u16,
                read_buffer_callback,
            )?
        }
        return_val = json_response_assembler.return_symbolicate_json_promise()?
    }
    Ok(return_val)
}

/// Deconstruct the "job" json object into a SymbolicateJob
fn parse_job(job: &JsonValue) -> SymbolicateResult<SymbolicateJob> {
    Ok(SymbolicateJob {
        memory_map: parse_memory_map(&job["memoryMap"])?,
        stacks: parse_job_stacks(&job["stacks"])?,
    })
}

/// Return the function name and function offset if found.
/// If no exact module offset is found, then the function uses the nearest offset rounded down. 
pub fn get_function_name(
    module_offset: u32,
    table: &CompactSymbolTable,
) -> SymbolicateResult<Option<SymbolicateFunctionInfo>> {
    let buffer_start : u32; 
    let buffer_end : u32; 
    match table.addr.binary_search(&module_offset) {
        Ok(found_index) => {
            buffer_start = table.index[found_index];
            buffer_end = table.index[found_index + 1];
        }
        // If not found, then we take the nearest rounded down index. 
        // possible_index returned by binary_search is the rounded up index, so we take
        // the one below it. Edge case would be when the possible_index is already 0, meaning the element
        // is indeed not found and there does not exist a nearest smaller element
        Err(possible_index) => {
            if possible_index != 0 {
                buffer_start = table.index[possible_index - 1];
                buffer_end = table.index[possible_index];
            } else {
                return Err(SymbolicateError::ModuleIndexOutOfBound(table.addr.len(), module_offset as usize));
            }
        }
    };

    Ok(Some(SymbolicateFunctionInfo {
        function: Some(
            std::str::from_utf8(&table.buffer[buffer_start as usize..buffer_end as usize])
                .unwrap()
                .to_string(),
        ),
        function_offset: Some((buffer_start - table.index[0]).to_string()),
    }))
}

fn parse_job_stacks(job_stacks: &JsonValue) -> SymbolicateResult<Vec<SymbolicateRequestStack>> {
    printFromRust("Printing from fxn parse_job_stacks".to_string());
    let mut result_vec = Vec::new();
    for (_, vec) in job_stacks
        .as_array()
        .ok_or_else(|| SymbolicateError::JsonParseArrayError)?
        .iter()
        .enumerate()
    {
        for (_, tuple) in vec
            .as_array()
            .ok_or_else(|| SymbolicateError::JsonParseArrayError)?
            .iter()
            .enumerate()
        {
            let tup_stack = SymbolicateRequestStack {
                module_index_and_offset: serde_json::from_value(tuple.clone()).unwrap(),
            };
            result_vec.push(tup_stack);
        }
    }
    result_vec.sort_by_key(|a| a.module_index_and_offset[0]);
    Ok(result_vec)
}

fn parse_memory_map(memory_map: &JsonValue) -> SymbolicateResult<Vec<SymbolicateMemoryMap>> {
    let mut result_vec = Vec::new();
    for (_, vec) in memory_map
        .as_array()
        .ok_or_else(|| SymbolicateError::JsonParseArrayError)?
        .iter()
        .enumerate()
    {
        let symbolicate_memory_map = SymbolicateMemoryMap {
            symbol_file_name: vec[0].to_string(),
            debug_id: vec[1].to_string(),
        };
        result_vec.push(symbolicate_memory_map);
    }
    Ok(result_vec)
}

pub fn get_symbolicate_response_json(
    symbolicate_resp: SymbolicateResponseJson,
) -> SymbolicateResult<JsValue> {
    let symbolicate_resp_json = symbolicate_resp.as_json();
    JsValue::from_serde(&symbolicate_resp_json).or_else(|err| Err(SymbolicateError::from(err)))
}

/// Assembles the symbolicate JSON for all the modules listed in the symbolicate request
pub struct SymbolicateJsonAssembler {
    total_modules: usize,
    pending_futures: Vec<
        Box<
            dyn futures::Future<
                Item = (SymbolicateResponseStack, usize, String),
                Error = SymbolicateError,
            >,
        >,
    >,
}

impl SymbolicateJsonAssembler {
    /// Requires the total number of modules requested so to instantiate the 
    /// capacity of the returning stacks with that, which the promises after
    /// JS callbacks can put the results in
    pub fn new(total_modules: usize) -> Self {
        SymbolicateJsonAssembler {
            total_modules: total_modules,
            pending_futures: Vec::new(),
        }
    }

    /// Process one job from the symbolicate request, and push that into the
    /// list of pending futures
    pub fn process_job_and_add_to_futures(
        &mut self,
        symbolicate_job: &SymbolicateJob,
        module_index: u16,
        read_buffer_callback: &js_sys::Function,
    ) -> SymbolicateResult<()> {
        let this = JsValue::NULL;
        let module_name = symbolicate_job.memory_map[module_index as usize]
            .symbol_file_name
            .to_string();
        let debug_id = JsValue::from(
            symbolicate_job.memory_map[module_index as usize]
                .debug_id
                .to_string(),
        );
        let module_offset = symbolicate_job.stacks[module_index as usize].get_module_offset();
        let module_name_id = symbolicate_job.memory_map[module_index as usize].as_string();
        let job_json_future = JsFuture::from(js_sys::Promise::from(
            read_buffer_callback
                .call2(&this, &JsValue::from(module_name.to_string()), &debug_id)
                .or_else(|err| Err(SymbolicateError::JsValueError(err)))?,
        ))
        .then(move |jsvalue| {
            if let Err(err) = jsvalue {
                return future::err(SymbolicateError::JsValueError(err));
            };
            let jsvalue_dyn = jsvalue.unwrap().dyn_into::<GetSymbolTableParamWrapper>();

            if let Err(err) = jsvalue_dyn {
                return future::err(SymbolicateError::JsValueError(err));
            }
            let table_buffers = jsvalue_dyn.unwrap();
            let binary_data: WasmMemBuffer = table_buffers.getInnerBinaryData();
            let debug_data: WasmMemBuffer = table_buffers.getInnerDebugData();
            let breakpad_id: String = table_buffers.getInnerDebugID();

            // instead of calling wasm_bindgen version to avoid serializing and deserializing error_msg
            let compact_symbol_table =
                get_compact_symbol_table_internal(&binary_data, &debug_data, &breakpad_id);

            if let Err(err) = compact_symbol_table {
                return future::err(SymbolicateError::CompactSymbolTableError(err));
            }

            let function_info = get_function_name(module_offset, &compact_symbol_table.unwrap());

            if let Err(err) = function_info {
                return future::err(err);
            }

            let mut response_stack: SymbolicateResponseStack = Default::default();
            response_stack.from(function_info.unwrap().unwrap());
            response_stack.module = module_name;
            response_stack.module_offset = format!("{:#x}", module_offset);
            response_stack.frame = module_index;
            future::ok((response_stack, module_index as usize, module_name_id))
        });
        self.pending_futures.push(Box::new(job_json_future));
        Ok(())
    }

    /// The function will return a JS Promise that returns when all job futures have been fulfilled
    /// function will consume self, so caller must ensure this is the last call on the assembler if used
    pub fn return_symbolicate_json_promise(
        self,
    ) -> SymbolicateResult<JsValue> {
        let mut symbolicate_resp = SymbolicateResponseResult::new(self.total_modules as usize);
        let pending_futures_join = futures::future::join_all(self.pending_futures);
        let futures_resolve = pending_futures_join
            .map(move |vs| {
                for (stack, module_index, module_name_id) in vs {
                    symbolicate_resp.found_modules.from(&stack, module_name_id);
                    if let Err(err) = symbolicate_resp.add_stack(stack, module_index) {
                        JsValue::from_serde(&SymbolicateErrorJson::from_error(err)).unwrap();
                    }
                }
                let symbolicate_resp_json = symbolicate_resp.as_json();
                JsValue::from_serde(&symbolicate_resp_json).unwrap()
            })
            .map_err(|err| JsValue::from_serde(&SymbolicateErrorJson::from_error(err)).unwrap());
        Ok(JsValue::from(future_to_promise(futures_resolve)))
    }
}

/// THIS IS A TEST TO SEE WHETHER RECURSIIVE AS_JSON CALL WILL WORK
/// Just for debugging uses
#[wasm_bindgen]
pub fn test_symb_json() -> JsValue {
    let mut result1 = SymbolicateResponseResult::new(2);
    let result2 = SymbolicateResponseResult::new(2);
    let result3 = SymbolicateResponseResult::new(2);

    let stack = SymbolicateResponseStack {
        module_offset: "0xb2e3f7".to_string(),
        module: "xul.pdb".to_string(),
        frame: 0,
        function: Some("KiUserCallbackDispatcher".to_string()),
        function_offset: None,
    };

    result1.add_stack(stack, 0);
    JsValue::from_serde(&result1.as_json()).unwrap()
}