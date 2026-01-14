# Step 3: Generate git-log.txt with the Full Log
From the repo root:
$ git --no-pager log --decorate --graph --stat > git-log.txt
This writes the full commit history (including full messages) from main into git-log.txt.

# Step 4: Create main.tgz with Project Files and git-log.txt
From the root of your repository:
$ git archive --format=tar.gz -o "../$(basename "$PWD")-main.tgz" --prefix="$
(basename "$PWD")/" main
This creates a gzip-compressed tar file in the parent directory with “-main.tgz” as the filename suffix.
This archive includes:
• Your project directory and all files
• The git-log.txt file created in Step 3
• No .git/ directory or Git metadata

