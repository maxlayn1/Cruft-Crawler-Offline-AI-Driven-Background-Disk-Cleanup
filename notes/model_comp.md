Short answers first:
- Smollm3-3B quantized to int8 (or "8-bit") will typically load in ~1.5–3 GB VRAM (model weights) depending on format/overhead; total VRAM while running with default context (~2048 tokens assumed) ≈ 3–5 GB if GPU holds most model + KV. CPU/GPU split reduces GPU VRAM further via offload.
- Fidelity for short tasks: 3B quantized → usually fine for short embedding/inference tasks and simple Q/A; some nuance or world-knowledge may degrade vs a 7B+ fp16 model, but for small-chunk embeddings/summaries it’s generally acceptable.
- Per-call compute is small; main cost is keeping the model loaded. Use persistent process + offload for best practical tradeoff.

Assumptions I used
- “Default context length” = 2048 tokens (common default for small LLMs).
- “Quantized 8” = 8-bit integer weight quantization (int8 or similar).
- Running mixed CPU/GPU with a loader/runtime that supports device_map/offload (e.g., Hugging Face accelerate, bitsandbytes + transformers, vLLM with offload, or llama.cpp variants).

Estimated memory & compute (approximate)
- Model weights (int8): ~0.5–1.0 bytes/parameter effective → for 3B params ≈ 1.5–3 GB on device. Practical packaged quant files are often ~1.5–2.5 GB.
- KV cache: for transformer decoder at FP16-equivalent activations, typical KV bytes ≈ 2 × hidden_size × seq_len × num_layers × bytes-per-value. Practical rule of thumb: ~0.4–0.8 MB per token for mid-sized models when kept in GPU FP16. For 2048 tokens that’s ≈ 0.8–1.6 GB. If offloaded to CPU, GPU KV reduces accordingly but CPU RAM must hold it.
- Working memory/activations/threads: ~200–800 MB extra depending on runtime and batch size.
- Total GPU VRAM if everything on GPU: ~model(int8) + KV + overhead ≈ 1.5–3 + 0.8–1.6 + 0.2–0.8 ≈ 2.5–5.4 GB. So your 4–8 GB estimate is reasonable.
- If using CPU + GPU split (device offload): GPU VRAM can drop to ~1–2 GB (just attention layers or key-heavy parts) while CPU RAM holds the rest (several GB).

Performance (throughput / latency)
- Single inference (generate short output, e.g., 50 tokens) after model loaded:
  - GPU (mid-range, e.g., 6–10 GB card): latency ~50–300 ms to produce final tokens (depends on runtime, batching, and generation strategy).
  - CPU-only (many cores, quantized): latency per call ~500 ms–several seconds.
- Embedding a small chunk (<512 tokens): typically tens of ms on GPU, 100–500 ms on CPU.

Fidelity tradeoffs (quantization & model size)
- Quantization to int8: small drop in quality for many tasks; often negligible for embeddings and short summarization/Q&A. Edge cases: subtle reasoning, arithmetic, or highly factual nuance may degrade.
- 3B vs 7B: 3B will be weaker on complex multi-step reasoning or knowledge-heavy generation. For short chunk embedding, classification, or extraction it’s usually sufficient.
- If you need deterministic high-fidelity answers for tricky prompts, prefer a larger or fp16 model; otherwise 3B int8 is a good cost/latency compromise.

Practical recommendations (concise)
1. Use an int8 quantized Smollm3-3B and keep it resident (avoid reloads).  
2. Use a runtime with offload/device_map (accelerate + bitsandbytes, vLLM, or llama.cpp if available) so GPU holds hot parts and CPU backs the rest.  
3. Pre-tokenize and batch small chunks into groups (e.g., 8–32) to improve throughput.  
4. Monitor GPU VRAM with a run and adjust offload settings; target 2–4 GB GPU usage for a comfortable margin on a 4–8 GB card.  
5. If you notice frequent hallucinations or missing nuance, switch to a 7B model or run small tasks with fp16 for critical cases.

If you want, I can compute a more exact VRAM/CPU estimate for your exact hardware (GPU model + VRAM, CPU cores/RAM) and intended max context and batch size — tell me that and I’ll calculate numbers.
