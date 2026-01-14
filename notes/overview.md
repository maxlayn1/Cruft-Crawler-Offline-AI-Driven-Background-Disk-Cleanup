# Program Path Overview
Crawl over given directory (is recursive)

send the File Metadata to the Database

Either Compute Hash or Vector Embedding of first 1024 bytes of a file
and send to either Sled or SahomeDB as part of Metadata

Only compute this for files of similar size. If we go for hashing, then 
add the conditional to check and add 't' to a boolean value to denote that the file is a duplicate

Contact the LLM with the given information, and File Metadata.
Do this in batches (have a variable denoting all the entries in a directory)
and then split up into batches of 10 or less. (group together duplicates)

Either have a short pre-prompt for every time you compute a batch or have a vector embedding or KV cache
for the pre-prompt. (minimize tokens and context window)

One of the largest problems is how to deal with the small context windows of these models
however our task is really simple, just give the model information on files and let it make one word responses
on what it the user should or should not do.
