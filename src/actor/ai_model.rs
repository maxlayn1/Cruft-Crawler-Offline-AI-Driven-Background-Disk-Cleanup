#![allow(unused)]

use steady_state::*;
use crate::llm_engine::LlmEngine;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::{AddBos, Special};
use llama_cpp_2::sampling::LlamaSampler;
use std::io::Write;
use std::num::NonZeroU32;
use std::{any, fs};

use crate::actor::crawler;

const MODEL_FILE_PATH: &str  = "./src/models/Llama-3.2-3B-Instruct-UD-Q4_K_XL.gguf";
const PROMPT_FILE_PATH: &str = "./src/prompt.txt";

// run function 
pub async fn run(actor: SteadyActorShadow, crawler_to_model_rx: SteadyRx<String>, ai_model_to_ui_tx: SteadyTx<String>) -> Result<(),Box<dyn Error>> {

    let actor = actor.into_spotlight([&crawler_to_model_rx], [&ai_model_to_ui_tx]);
	

	if actor.use_internal_behavior {
	    internal_behavior(actor, crawler_to_model_rx, ai_model_to_ui_tx).await
	} else {
	    actor.simulated_behavior(vec!(&crawler_to_model_rx)).await
	}
}


// Internal behaviour for the actor
// @TO-DO:
// - Change data types of channels other than string
async fn internal_behavior<A: SteadyActor>(mut actor: A, crawler_to_ai_model_rx: SteadyRx<String>, ai_model_to_ui_tx: SteadyTx<String>) -> Result<(),Box<dyn Error>> {
	
	let mut crawler_to_ai_model_rx = crawler_to_ai_model_rx.lock().await;
	let mut ai_model_to_ui_tx = ai_model_to_ui_tx.lock().await;

	let engine = LlmEngine::load_new_model(
        MODEL_FILE_PATH
    )?;

    let prompt1 = fs::read_to_string(PROMPT_FILE_PATH)?;
    let resp1 = engine.infer_model(&prompt1)?;
    println!("Response 1:\n{}", resp1);
   
    
    while actor.is_running(|| crawler_to_ai_model_rx.is_closed_and_empty() || ai_model_to_ui_tx.mark_closed()) {
		
		await_for_all!(actor.wait_avail(&mut crawler_to_ai_model_rx, 1), actor.wait_vacant(&mut ai_model_to_ui_tx, 1));

		let resp1 = engine.infer_model(&prompt1)?;
		
	    //Reciecing data from crawler actor
	    //actor.wait_avail(&mut crawler_to_ai_model_rx, 1).await;
        let recieved = actor.try_take(&mut crawler_to_ai_model_rx).expect("ai actor failed to take from crawler");

		//Sending data to ui actor
		//actor.wait_vacant(&mut ai_model_to_ui_tx, 1).await;
		let ai_to_ui_message = "Hello";
    } 

	return Ok(());
}