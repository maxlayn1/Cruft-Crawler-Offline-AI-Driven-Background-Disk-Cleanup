#![allow(unused)]
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, Special};
use llama_cpp_2::sampling::LlamaSampler;
use std::io::Write;
use std::num::NonZeroU32;
use std::{any, fs};
use llama_cpp_2::{send_logs_to_tracing,LogOptions};

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
                libc::CPU_SET(0, &mut cpu_set); //pin to core 0, seems to overfill to cores 1, and 2
                libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &cpu_set);
            }
        }

        Ok(Self { backend, model })
    }
    
    fn create_context(&self) -> anyhow::Result<LlamaContext<'_>> {
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(NonZeroU32::new(2048).unwrap())) //IT CANNOT HANDLE 128 CONTEXT SIZE
            .with_n_threads(1)
            .with_n_threads_batch(1); //attempt to keep it to one thread

        let ctx = self.model.new_context(&self.backend, ctx_params)?;
        Ok(ctx)
    }

    pub fn infer_model(&self, prompt: &str) -> anyhow::Result<String> {
        let mut ctx = self.create_context()?;
        let tokens = self.model.str_to_token(prompt, AddBos::Always)?;

        // --- tunable knobs ---
        let chunk_size: usize = 1;
        let chunk_delay = Duration::from_millis(1250);
        let gen_delay = Duration::from_millis(30000);
        let max_tokens = 20;
        // ----------------------
        

        let mut batch = LlamaBatch::new(64, 1);
        let total = tokens.len();
        let chunks = tokens.chunks(chunk_size);
        let num_chunks = (total + chunk_size - 1) / chunk_size;

        let mut last_chunk_len: i32 = 0;

        for (chunk_idx, chunk) in chunks.enumerate() {
            let is_last_chunk = chunk_idx == num_chunks - 1;

            for (i, &token) in chunk.iter().enumerate() {
                let pos = (chunk_idx * chunk_size + i) as i32;
                let needs_logits = is_last_chunk && i == chunk.len() - 1;
                batch.add(token, pos, &[0], needs_logits)?;
            }

            ctx.decode(&mut batch)?;
            last_chunk_len = chunk.len() as i32;
            batch.clear();

            if !is_last_chunk {
                sleep(chunk_delay);
            }
        }

        // n_cur should now reflect total prompt tokens processed
        let mut n_cur = total as i32;

        // Minimal sampler (lower CPU than chain_simple)
        let mut sampler = LlamaSampler::greedy();

        // UTF-8 decoder
        let mut decoder = encoding_rs::UTF_8.new_decoder();

        // Logits index = last token of final chunk
        let mut logits_idx = last_chunk_len - 1;

        let mut response = String::new();

        for _ in 0..max_tokens {
            let token = sampler.sample(&ctx, logits_idx);
            sampler.accept(token);

            if self.model.is_eog_token(token) {
                break;
            }

            let output_bytes = self.model.token_to_bytes(token, Special::Tokenize)?;
            let mut output_string = String::with_capacity(32);
            decoder.decode_to_string(&output_bytes, &mut output_string, false);

            //print!("{}", output_string);
            //std::io::stdout().flush()?;

            response.push_str(&output_string);

            // Prepare next iteration
            batch.clear();
            batch.add(token, n_cur, &[0], true)?;
            n_cur += 1;

            logits_idx = 0; // single token batch

            ctx.decode(&mut batch)?;

            // throttle generation
            if !gen_delay.is_zero() {
                sleep(gen_delay);
            }
        }

        decoder.decode_to_string(b"", &mut response, true);
        self.write_response_to_file(&response)?;
        Ok(response)
    }

    fn write_response_to_file(&self, response: &str) -> anyhow::Result<()> {
        //Write responses to output file
        let mut file = fs::OpenOptions::new()
            .append(true) // append mode
            .create(true) // create if it doesn't exist
            .open("./LLM_responses.txt")?;
        writeln!(file, "{}", response)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Read;

    // ── helpers ───────────────────────────────────────────────────────────────
    // We can't construct a real LlmEngine without a .gguf model file, so we
    // extract write_response_to_file's logic into a standalone helper to test
    // the I/O behavior directly.

    fn write_response(path: &str, response: &str) -> anyhow::Result<()> {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)?;
        writeln!(file, "{}", response)?;
        Ok(())
    }

    fn temp_path(name: &str) -> String {
        std::env::temp_dir()
            .join(name)
            .to_string_lossy()
            .to_string()
    }

    fn cleanup(path: &str) {
        let _ = fs::remove_file(path);
    }

    // ── write_response_to_file behavior ──────────────────────────────────────

    #[test]
    fn test_write_creates_file_if_not_exists() {
        let path = temp_path("llm_test_create.txt");
        cleanup(&path);

        write_response(&path, "Decision: delete").unwrap();

        assert!(fs::metadata(&path).is_ok(), "file should have been created");
        cleanup(&path);
    }

    #[test]
    fn test_write_content_is_correct() {
        let path = temp_path("llm_test_content.txt");
        cleanup(&path);

        write_response(&path, "Decision: keep").unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("Decision: keep"));
        cleanup(&path);
    }

    #[test]
    fn test_write_appends_not_overwrites() {
        let path = temp_path("llm_test_append.txt");
        cleanup(&path);

        write_response(&path, "first response").unwrap();
        write_response(&path, "second response").unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("first response"));
        assert!(content.contains("second response"));
        cleanup(&path);
    }

    #[test]
    fn test_write_adds_newline_after_response() {
        let path = temp_path("llm_test_newline.txt");
        cleanup(&path);

        write_response(&path, "some output").unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.ends_with('\n'), "writeln! should append a newline");
        cleanup(&path);
    }

    #[test]
    fn test_write_empty_string() {
        let path = temp_path("llm_test_empty.txt");
        cleanup(&path);

        write_response(&path, "").unwrap();

        let content = fs::read_to_string(&path).unwrap();
        // writeln!("") produces just a newline
        assert_eq!(content, "\n");
        cleanup(&path);
    }

    #[test]
    fn test_write_multiple_lines_accumulated() {
        let path = temp_path("llm_test_multi.txt");
        cleanup(&path);

        let responses = vec!["alpha", "beta", "gamma"];
        for r in &responses {
            write_response(&path, r).unwrap();
        }

        let content = fs::read_to_string(&path).unwrap();
        for r in &responses {
            assert!(content.contains(r));
        }
        assert_eq!(content.lines().count(), 3);
        cleanup(&path);
    }

    #[test]
    fn test_write_special_characters() {
        let path = temp_path("llm_test_special.txt");
        cleanup(&path);

        let response = "Decision: delete | /tmp/cache~.bak | size=204 bytes";
        write_response(&path, response).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("Decision: delete"));
        assert!(content.contains("/tmp/cache~.bak"));
        cleanup(&path);
    }

    #[test]
    fn test_write_unicode_content() {
        let path = temp_path("llm_test_unicode.txt");
        cleanup(&path);

        write_response(&path, "файл удалить").unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("файл удалить"));
        cleanup(&path);
    }

    #[test]
    fn test_write_to_invalid_path_returns_error() {
        // A path inside a nonexistent directory should fail
        let result = write_response("/nonexistent_dir_xyz/output.txt", "test");
        assert!(result.is_err());
    }

    // ── load_new_model: invalid path returns error ────────────────────────────
    // This is the only load_new_model path we can test without the .gguf file.

    #[test]
    fn test_load_new_model_nonexistent_path_returns_error() {
        let result = LlmEngine::load_new_model("/nonexistent/path/model.gguf");
        assert!(result.is_err(), "loading a missing model file should fail");
    }
}