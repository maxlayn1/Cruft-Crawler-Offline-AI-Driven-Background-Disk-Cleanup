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
use std::time::Duration;
use std::thread::sleep;

///config for CPU optimization and throttling
pub struct ThrottleConfig {
    pub chunk_size: usize,
    pub chunk_delay: Duration,
    pub gen_delay: Duration,
    pub max_tokens: usize,
    pub cpu_core: Option<usize>,                                                                    //none = don't pin, Some(n) = pin to core n
    pub nice_value: Option<i32>,                                                                    //none = don't set, Some(n) = set nice value
}

impl Default for ThrottleConfig {
    fn default() -> Self {
        Self {
            chunk_size: 1,                                                                            //1 token per chunk = minimal CPU spikes
            chunk_delay: Duration::from_millis(1250),                                                 //~2.5 min for 120 tokens
            gen_delay: Duration::from_millis(30000),                                                  //30s between generated tokens
            max_tokens: 20,
            cpu_core: Some(None),                                                                     //keeping no pin and no NICEness for now
            nice_value: Some(None),                                                                   
        }
    }
}
pub struct LlmEngine {
    backend: LlamaBackend,
    model:   LlamaModel,
    throttle_config: ThrottleConfig,
}

impl LlmEngine {
    pub fn load_new_model(model_path: &str) -> anyhow::Result<Self> {
        Self::load_new_model_with_config(model_path, ThrottleConfig::default())
    }

    pub fn load_new_model_with_config(
        model_path: &str,
        throttle_config: ThrottleConfig,
    ) -> anyhow::Result<Self> {
        //apply CPU pinning and nice value if configured 
        #[cfg(target_os = "linux")]
        {
            if let Some(core) = throttle_config.cpu_core {
                unsafe {
                    let mut cpu_set: libc::cpu_set_t = std::mem::zeroed();
                    libc::CPU_SET(core, &mut cpu_set);
                    libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &cpu_set);
                }
            }

            if let Some(nice) = throttle_config.nice_value {
                unsafe {
                    libc::setpriority(libc::PRIO_PROCESS, 0, nice);
                }
            }
        }

        let backend = LlamaBackend::init()?;
        let model_params = LlamaModelParams::default();

        let model = LlamaModel::load_from_file(&backend, model_path, &model_params)?;

        Ok(Self {
            backend,
            model,
            throttle_config,
        })
    }

    fn create_context(&self)-> anyhow::Result<LlamaContext<'_>>{
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(NonZeroU32::new(128).unwrap()))                        //context size reduced from 2056 to 128
            .with_n_threads(1)
            .with_n_threads_batch(1);                                                       //setting it to one core, could potentially break with steady_state!

        let ctx = self.model.new_context(&self.backend, ctx_params)?;
        Ok(ctx)
    }

    fn tokenize_prompt(&self, prompt: &str) -> anyhow::Result<Vec<llama_cpp_2::token::LlamaToken>> {
    Ok(self.model.str_to_token(prompt, AddBos::Always)?)
    }

    pub fn infer_model(&self, prompt: &str)-> anyhow::Result<String>{
        let mut ctx = self.create_context()?;
        let mut batch = self.tokenize_prompt(prompt)?;

        ctx.decode(&mut batch)?; 

        let mut sampler = LlamaSampler::greedy();                                       //swapped to most minimal possible sampler to reduce CPU load
            
        let mut decoder = encoding_rs::UTF_8.new_decoder();                                  //UTF-8 encoding crate should work here
        let mut response = String::new();
        let mut n_cur = batch.n_tokens();
        let max_tokens = 100;

        // logits index: points to where logits were requested within the last decoded batch.
        // after prompt: last token of the final chunk. During generation: always 0 (single-token batch).
        //let mut logits_idx = last_chunk_len - 1;

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
