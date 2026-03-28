#![allow(unused)]

use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, Special};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::{send_logs_to_tracing, LogOptions};

use std::io::Write;
use std::num::NonZeroU32;
use std::fs;

use std::thread::sleep;
use std::time::Duration;

pub struct LlmEngine {
    backend: LlamaBackend,
    model: LlamaModel,
}

impl LlmEngine {
    pub fn load_new_model(model_path: &str) -> anyhow::Result<Self> {
        let backend = LlamaBackend::init()?;
        let model_params = LlamaModelParams::default();
        let log_options = LogOptions::default().with_logs_enabled(true);
        send_logs_to_tracing(log_options);

        let model = LlamaModel::load_from_file(&backend, model_path, &model_params)?;

        #[cfg(target_os = "linux")]
        {
            unsafe {
                let mut cpu_set: libc::cpu_set_t = std::mem::zeroed();
                libc::CPU_SET(0, &mut cpu_set);
                libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &cpu_set);
            }
        }

        Ok(Self { backend, model })
    }

    fn create_context(&self) -> anyhow::Result<LlamaContext<'_>> {
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(NonZeroU32::new(256).unwrap()))
            .with_n_threads(1)
            .with_n_threads_batch(1);

        let ctx = self.model.new_context(&self.backend, ctx_params)?;
        Ok(ctx)
    }

    pub fn infer_model(&self, prompt: &str) -> anyhow::Result<String> {
        let mut ctx = self.create_context()?;
        let tokens = self.model.str_to_token(prompt, AddBos::Always)?;

        // --- tunable knobs ---
        let gen_delay = Duration::from_millis(100);
        let max_tokens = 20;
        // ----------------------

        // Feed the entire prompt in one batch; request logits only on last token
        let mut batch = LlamaBatch::new(tokens.len().max(64), 1);

        for (i, &token) in tokens.iter().enumerate() {
            let is_last = i == tokens.len() - 1;
            batch.add(token, i as i32, &[0], is_last)?;
        }

        // Decode prompt once
        ctx.decode(&mut batch)?;

        // Generation state
        let mut n_cur = tokens.len() as i32;
        let mut sampler = LlamaSampler::greedy();
        let mut decoder = encoding_rs::UTF_8.new_decoder();

        // logits are on the last token of the prompt batch
        let mut logits_idx: i32 = (tokens.len() as i32) - 1;
        let mut response = String::new();

        // Reuse batch for generation tokens
        batch.clear();

        for _ in 0..max_tokens {
            let token = sampler.sample(&ctx, logits_idx);
            sampler.accept(token);

            if self.model.is_eog_token(token) {
                break;
            }

            let output_bytes = self.model.token_to_bytes(token, Special::Tokenize)?;
            let mut output_string = String::with_capacity(32);
            decoder.decode_to_string(&output_bytes, &mut output_string, false);
            response.push_str(&output_string);

            batch.clear();
            batch.add(token, n_cur, &[0], true)?;
            n_cur += 1;

            // after decoding a single-token batch, logits index is 0
            logits_idx = 0;

            ctx.decode(&mut batch)?;

            if !gen_delay.is_zero() {
                sleep(gen_delay);
            }
        }

        decoder.decode_to_string(b"", &mut response, true);
        self.write_response_to_file(&response)?;
        Ok(response)
    }

    fn write_response_to_file(&self, response: &str) -> anyhow::Result<()> {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open("./LLM_responses.txt")?;
        writeln!(file, "{}", response)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_load() {
        let path = r"C:\cc\src\models\llama-3.2-3b-instruct-q8_0.gguf";
        println!("Loading model from: {}", path);
        match LlmEngine::load_new_model(path) {
            Ok(_) => println!("SUCCESS: model loaded"),
            Err(e) => println!("FAILED: {}", e),
        }
    }
}