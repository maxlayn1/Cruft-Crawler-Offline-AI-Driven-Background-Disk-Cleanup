# Cruft-Crawler
Cruft Crawler is an LLM-first background agent that runs entirely offline from a 64 GB USB drive. It profiles the filesystem slowly over time with imperceptible CPU load.
It uses a local quantized LLM to help recommend safe deletions, and delivers a concise AI-generated report.

# TODO 

## crawler actor
- [ ] TODO: get absolute path of file name for id
- [ ] TODO: import hashing crate and hash first chunk of files or vector embedding
- [ ] TODO: hard-code values for different file-types and how to treat them
- [ ] TODO: implement Walkdir to recursively get different directories
- [ ] TODO: Implement state or communication to Database to ensure its crawling in correct location on actor failure
- [ ] TODO: see about replacing SystemTime, with another field for better parsing

## db_manager actor
- [ ] TODO: Remove SahomeDB, use Sled instead
- [ ] TODO: push all the metadata into the Sled database 
- [ ] TODO: research a way to view the sled database for presentation
- [ ] TODO: create DB schema for Sled / SahomeDB
- [ ] TODO: Implement state or communication to Database to ensure its crawling in correct location on actor failure

## (stretch) implement the llama.cpp actor into the prototype
- [ ] TODO: make Max's llama code actor compliant
- [ ] TODO: port over Max's llama actor
- [ ] TODO: get vector embeddings of first chunk of file and put into DB?


# Crates Potentially needed
filetime: https://docs.rs/filetime/latest/filetime/ (cross-platform time-dates)
Walkdir: https://docs.rs/walkdir/latest/walkdir/ (for recursive traversal of file-system)
sled: https://docs.rs/sled/latest/sled/ (for long term storage)
steady-state: https://docs.rs/steady_state/latest/steady_state/ (project architecture)
llama-cpp: https://docs.rs/llama_cpp/latest/llama_cpp/ (interact with llama-cpp bindings for LLM)
