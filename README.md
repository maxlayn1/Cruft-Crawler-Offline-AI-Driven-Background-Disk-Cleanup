# Cruft-Crawler
Cruft Crawler is an LLM-first background agent that runs entirely offline from a 64 GB USB drive. It profiles the filesystem slowly over time with imperceptible CPU load.
It uses a local quantized LLM to help recommend safe deletions, and delivers a concise AI-generated report.

# TODO (final presentation)

## crawler actor
- [ ] TODO: import hashing crate and hash first chunk of files
- [ ] TODO: hard-code values for different file-types and how to treat them
- [ ] TODO: implement Walkdir to recursively get different directories
- [ ] TODO: Implement state or communication to Database to ensure its crawling in correct location on actor failure

## db_manager actor
- [ ] TODO: Remove SahomeDB, use Sled instead
- [ ] TODO: push all the metadata into the Sled database 
- [ ] TODO: research a way to view the sled database for presentation

## (stretch) implement the llama.cpp actor into the prototype
- [ ] TODO: make Max's llama code actor compliant
- [ ] TODO: port over Max's llama actor
