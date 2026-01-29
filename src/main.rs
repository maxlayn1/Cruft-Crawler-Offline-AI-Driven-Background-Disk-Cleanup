use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::{AddBos, Special};
use llama_cpp_2::sampling::LlamaSampler;
use std::io::Write;
use std::num::NonZeroU32;

fn main() -> anyhow::Result<()> {
    // Initialize the backend
    let backend = LlamaBackend::init()?;

    // Set up model parameters
    let model_params = LlamaModelParams::default();
    
    // Load the model
    let model = LlamaModel::load_from_file(
        &backend,
        "models/Llama-3.2-3B-Instruct-Q4_K_M.gguf",
        &model_params
    )?;

    // Create context with 2048 token context size
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(Some(NonZeroU32::new(2048).unwrap()));
    
    let mut ctx = model.new_context(&backend, ctx_params)?;

    // The prompt
    let prompt = "The sky is blue. Is this statement true or false? Do not output anything besides either TRUE or FALSE under ANY circumstances.";
    
    // Tokenize the prompt
    let tokens = model.str_to_token(prompt, AddBos::Always)?;
    
    println!("Prompt: {}", prompt);
    println!("Generating response...\n");

    // Create a batch and add all prompt tokens
    let mut batch = LlamaBatch::new(512, 1);
    let last_index = (tokens.len() - 1) as i32;
    
    for (i, token) in tokens.into_iter().enumerate() {
        let is_last = i as i32 == last_index;
        batch.add(token, i as i32, &[0], is_last)?;
    }

    // Process the prompt
    ctx.decode(&mut batch)?;

    // Set up sampler (greedy sampling - always picks most likely token)
    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::dist(1234), // seed
        LlamaSampler::greedy(),
    ]);

    // Generate tokens
    let max_tokens = 100;
    let mut n_cur = batch.n_tokens();
    
    // Decoder for handling UTF-8 properly
    let mut decoder = encoding_rs::UTF_8.new_decoder();

    for _ in 0..max_tokens {
        // Sample the next token
        let token = sampler.sample(&ctx, batch.n_tokens() - 1);
        sampler.accept(token);

        // Check for end of generation
        if model.is_eog_token(token) {
            println!();
            break;
        }

        // Convert token to bytes and then to string
        let output_bytes = model.token_to_bytes(token, Special::Tokenize)?;
        let mut output_string = String::with_capacity(32);
        decoder.decode_to_string(&output_bytes, &mut output_string, false);
        
        print!("{}", output_string);
        std::io::stdout().flush()?;

        // Prepare next iteration
        batch.clear();
        batch.add(token, n_cur, &[0], true)?;
        n_cur += 1;

        ctx.decode(&mut batch)?;
    }

    println!("\n");
    
    Ok(())
}