# Cruft Crawler

**Offline AI-driven background disk cleanup** — Cruft Crawler scans your filesystem, hashes file contents, runs a fully local GGUF language model to decide whether each file looks safe to delete, and presents the results in a terminal UI for human review before anything is removed.

---

## How It Works

The app is organized as an actor pipeline with four components wired together through `steady_state` channels:

| Actor | Role |
|---|---|
| `crawler` | Walks the filesystem, extracts metadata (path, size, timestamps, read-only flag), and computes SHA-256 hashes |
| `ai_model` | Loads a local GGUF model, builds a prompt from file metadata, and produces a `keep` or `delete` verdict |
| `db_manager` | Writes file metadata to a local `sled` database and deletes files from disk after user confirmation |
| `user_interface` | Runs a Ratatui terminal UI and forwards confirmed deletions back to the DB actor |

The AI model **always prefers keeping** files when uncertain — it only suggests deletion when the file name, age, size, and read-only status together indicate it is safe to remove.

---

## Requirements

- Rust toolchain (`rustup`, `cargo`, `rustc`)
- LLVM/Clang 17+ with `libclang` (required to compile `llama_cpp_2`)
- A `.gguf` model file placed in `./src/models/`
- A terminal capable of running the Ratatui text UI

---

## Choosing a GGUF Model

Cruft Crawler runs inference fully offline using any GGUF-format model. You can download compatible models directly from Hugging Face.

**Recommended model pages on Hugging Face:**

- [Llama 3.2 3B Instruct GGUF (unsloth)](https://huggingface.co/unsloth/Llama-3.2-3B-Instruct-GGUF) — good balance of speed and quality; the project was built and tested with this model
- [Llama 3.2 1B Instruct GGUF (unsloth)](https://huggingface.co/unsloth/Llama-3.2-1B-Instruct-GGUF) — faster, lower RAM usage, suitable for older or low-spec hardware
- [Phi-3.5 Mini Instruct GGUF (microsoft)](https://huggingface.co/microsoft/Phi-3.5-mini-instruct-gguf) — very capable small model from Microsoft
- [Mistral 7B Instruct GGUF (TheBloke)](https://huggingface.co/TheBloke/Mistral-7B-Instruct-v0.2-GGUF) — stronger reasoning, requires more RAM (~6–8 GB)

**Which quantization to pick:**

On each Hugging Face model page you will find multiple `.gguf` files with names like `Q4_K_M`, `Q5_K_M`, `Q8_K_XL`, etc. These are different compression levels:

| Quantization | RAM usage | Quality |
|---|---|---|
| `Q4_K_M` | ~2–3 GB | Good — recommended for most machines |
| `Q5_K_M` | ~3–4 GB | Better quality, slightly more RAM |
| `Q8_K_XL` | ~5–6 GB | Near-full quality, needs more RAM |

If you are unsure, download the `Q4_K_M` variant of whatever model you choose.

Once downloaded, place the `.gguf` file into `./src/models/` inside the project root. Cruft Crawler automatically loads the first `.gguf` file it finds in that folder — no config change needed.

---

## Setting Directories to Scan

Cruft Crawler reads the list of directories to scan from a plain text file called **`scan_paths.txt`** located in the project root.

### How to edit it

Open `scan_paths.txt` in any text editor. Each line is one absolute directory path. For example:

**Windows:**
```
C:\Users\YourName\Downloads
C:\Users\YourName\Documents\old_projects
D:\Backups
```

**Linux / macOS:**
```
/home/yourname/Downloads
/home/yourname/Documents/old_projects
/mnt/backup
```

**Rules:**
- One path per line
- Blank lines and lines starting with `#` are treated as comments and ignored
- Paths must be absolute (starting from the drive root on Windows or `/` on Linux/macOS)
- The crawler skips `target/` build directories automatically to avoid scanning Rust build artifacts

Save the file and run the app — the crawler will walk every directory listed when it starts.

> **Note:** The current source code has a hardcoded scan path as a placeholder (`C:\Users\tiger\Downloads`). If `scan_paths.txt` is not yet wired up in your build, edit that path directly in `src/actor/crawler.rs` in the `internal_behavior` function until the file-based approach is implemented.

---

## Windows — Install & Build from Source

> **No pre-built Windows executable is available. You must build the project yourself.**

### 1. Install prerequisites

Install all of the following before proceeding:

- [Git for Windows](https://git-scm.com/download/win)
- [Rust via rustup](https://rustup.rs/) — when prompted, select the default `x86_64-pc-windows-msvc` toolchain
- [LLVM 17+](https://releases.llvm.org/) — install to `C:\Program Files\LLVM` and check **"Add LLVM to PATH"** during setup
- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) — install with the **Desktop development with C++** workload selected

### 2. Clone the repository

Open PowerShell and run:

```powershell
git clone https://github.com/maxlayn1/Cruft-Crawler-Offline-AI-Driven-Background-Disk-Cleanup.git
cd Cruft-Crawler-Offline-AI-Driven-Background-Disk-Cleanup
```

### 3. Enable long paths

Windows has a default path length limit that can cause build failures. Run these in PowerShell **as Administrator**:

```powershell
reg add "HKLM\SYSTEM\CurrentControlSet\Control\FileSystem" /v LongPathsEnabled /t REG_DWORD /d 1 /f
git config --system core.longpaths true
```

### 4. Set LIBCLANG_PATH

```powershell
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
```

Adjust the path if LLVM is installed in a different location. This environment variable must be set in every new PowerShell session before building, or add it permanently via **System Properties → Environment Variables → System variables**.

### 5. Add your GGUF model

```powershell
mkdir src\models
copy C:\path\to\your-model.gguf src\models\
```

See the [Choosing a GGUF Model](#choosing-a-gguf-model) section above for download links.

### 6. Set your scan directories

Edit `scan_paths.txt` in the project root and add the directories you want scanned, one per line. See [Setting Directories to Scan](#setting-directories-to-scan) above.

### 7. Build the project

```powershell
cargo build --release
```

This will take several minutes on first build as Cargo compiles all dependencies including the LLM backend.

### 8. Run

```powershell
cargo run --release
```

---

## Linux — Run with Executable

> These steps assume you already have a compiled Linux binary.

### 1. Place the binary

```bash
mkdir -p ~/cruft-crawler
mv cruft-crawler ~/cruft-crawler/
cd ~/cruft-crawler
```

### 2. Make it executable

```bash
chmod +x cruft-crawler
```

### 3. Add your GGUF model

```bash
mkdir -p src/models
cp /path/to/your-model.gguf src/models/
```

See the [Choosing a GGUF Model](#choosing-a-gguf-model) section above for download links.

### 4. Set your scan directories

Create a `scan_paths.txt` file in the same directory as the executable and add your target directories, one per line:

```bash
nano scan_paths.txt
```

Example contents:

```
/home/yourname/Downloads
/home/yourname/Documents/old_projects
```

### 5. Run

```bash
./cruft-crawler
```

---

## Terminal UI Controls

Once running, the terminal UI shows files suggested for deletion and lets you review them one by one:

| Key | Action |
|---|---|
| `↑` / `↓` | Navigate the file list |
| `d` | Delete the selected file |
| `k` | Keep the selected file |
| `n` | Mark file as never-delete |
| `q` | Quit |

---

## Running Tests

```bash
# Run the full test suite
cargo test

# Run tests for a specific module
cargo test crawler
cargo test ai_model
cargo test db_manager
cargo test user_interface
cargo test llm_engine
```

---

## Known Limitations

- The crawler scan path is currently hardcoded in the source as a placeholder and should be replaced with `scan_paths.txt` file reading before general use.
- The local database is written to `./src/db`, which means runtime data lives inside the source tree.
- `file_handler.rs` is an older stub and is not part of the active runtime — it can be safely deleted.

---

## License

MIT
