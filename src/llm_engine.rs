#![allow(unused)]
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::{AddBos, Special};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::context::LlamaContext;
use std::io::Write;
use std::num::NonZeroU32;
use std::{any, fs};

pub struct LlmEngine {
    backend: LlamaBackend,
    model:   LlamaModel,
}

impl LlmEngine{
    pub fn load_new_model(model_path: &str)-> anyhow::Result<Self>{
        let backend = LlamaBackend::init()?;
        let model_params = LlamaModelParams::default();

        let model = LlamaModel::load_from_file(
            &backend,
            model_path,
            &model_params,
        )?;

        Ok(Self { backend, model })
    }

    fn create_context(&self)-> anyhow::Result<LlamaContext<'_>>{
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(NonZeroU32::new(128).unwrap()))   // context size reduced from 2056 to 128
            .with_n_threads(1)
            .with_n_threads_batch(1);   // setting it to one core, could potentially break with steady_state!

        let ctx = self.model.new_context(&self.backend, ctx_params)?;
        Ok(ctx)
    }

    fn tokenize_prompt(&self, prompt: &str) -> anyhow::Result<LlamaBatch<'_>>{
        let tokens = self.model.str_to_token(prompt, AddBos::Always)?;

        let mut batch = LlamaBatch::new(64,1);
        let last_index = (tokens.len() -1) as i32;

        for(i, token) in tokens.into_iter().enumerate(){
            batch.add(token, i as i32, &[0], i as i32 == last_index)?;
        }

        Ok(batch)
    }

    pub fn infer_model(&self, prompt: &str)-> anyhow::Result<String>{
        let mut ctx = self.create_context()?;
        let mut batch = self.tokenize_prompt(prompt)?;

        ctx.decode(&mut batch)?; 

        // Set up sampler (greedy sampling - always picks most likely token)
	    let mut sampler = LlamaSampler::chain_simple([
	        LlamaSampler::dist(10), // seed
	        LlamaSampler::greedy(),
	    ]);
            
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut response = String::new();
        let mut n_cur = batch.n_tokens();
        let max_tokens = 100;

        for _ in 0..max_tokens {
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);

            if self.model.is_eog_token(token) {
                break;
            }

            let bytes = self.model.token_to_bytes(token, Special::Tokenize)?;

            // Simple debug: print as you go
            if let Ok(text) = std::str::from_utf8(&bytes) {
                print!("{text}");
                response.push_str(text);
            }

            let mut output_string = String::new();
            decoder.decode_to_string(&bytes, &mut output_string, true);

            batch.clear();
            batch.add(token, n_cur, &[0], true)?;
            n_cur += 1;
            ctx.decode(&mut batch)?;
        }
        decoder.decode_to_string(b"", &mut response, true);
        self.write_response_to_file(&response)?;
        Ok(response)
    }

    fn write_response_to_file(&self, response: &str)-> anyhow::Result<()>{
        //Write responses to output file
        let mut file = fs::OpenOptions::new()
            .append(true)   // append mode
            .create(true)   // create if it doesn't exist
            .open("./LLM_responses.txt")?;
        writeln!(file, "{}", response)?;
        Ok(())
    }
}
