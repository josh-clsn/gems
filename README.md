# Gems - Autonomi Network Uploader/Archiver/Downloader

`gems` is a command-line tool written in Rust to interact with the Autonomi network. It allows you to:

*   Upload files.
*   Optionally verify uploads by downloading the data immediately.
*   Optionally create a `PublicArchive`.
*   Create a `PublicArchive` for a file already on the network using its `DataAddress`.
*   Download single files using their `DataAddress`.
*   Download all files referenced within a `PublicArchive` using its `ArchiveAddress`.

## Prerequisites

*   **Rust Toolchain:** Ensure you have Rust and Cargo installed. If not, follow the instructions at [rustup.rs](https://rustup.rs/). Cargo is included with Rust.
*   **Git:** Needed to clone the repository.
*   **ETH Wallet:** You need an ETH wallet private key with sufficient funds ANT & ETH on the mainnet to pay for uploads and archive creation.

## Installation

1.  **Clone the Repository:**
    ```bash
    git clone git@github.com:josh-clsn/gems.git
    cd gems
    ```

2.  **Create `.env` file:** In the root directory of the project (where `Cargo.toml` is), create a file named `.env`.

3.  **Add Private Key:** Add your ETH private key (in hex format, usually without the `0x` prefix) to the `.env` file:
    ```dotenv
    AUTONOMI_PRIVATE_KEY=<your_actual_private_key_hex>
    ```
    **IMPORTANT:** Keep this file secure and **never commit it to version control**. Add `.env` to your `.gitignore` file.

4.  **Install using Cargo:** This compiles the tool and places the `gems` executable in your Cargo binary path (`~/.cargo/bin/` by default), making it available as a command.
    ```bash
    cargo install --path .
    ```
    *Ensure that `~/.cargo/bin` is in your system's `PATH` environment variable. If you just installed Rust, you might need to restart your terminal or run `source ~/.cargo/env`.* 

## Usage

Once installed, you can run the tool using the `gems` command followed by a subcommand (`upload`, `archive`, or `download`).

### 1. Uploading a File (`upload`)

This command uploads a local file to the network.

```bash
gems upload --file-path <path/to/your/local/file>
# Example:
gems upload -f ./my_document.pdf
```

**Workflow:**

1.  The tool reads the local file.
2.  **It asks you upfront** if you want to **download and verify** the data *after* the upload completes successfully.
3.  **It asks you upfront** if you want to **create a new archive** for the data *after* the upload completes successfully.
4.  The tool then attempts the upload, retrying up to 50 times if it fails. You can leave the process running.
5.  If the upload is successful, it prints the **`Data Address`**.
6.  Based on your earlier answers:
    *   If you requested verification, it proceeds to download and verify the data.
    *   If you requested archiving, it proceeds to create and upload the `PublicArchive` structure, printing the **`Archive Address`**.

**Optional Output Directory for Verification:**

You can specify where the verified file should be saved (if verification is performed) using `-o` or `--output-dir` (defaults to the current directory).

```bash
gems upload -f ./image.jpg -o ./verified_downloads
# (It will still ask the verify/archive questions before uploading)
```

### 2. Creating an Archive for Existing Data (`archive`)

Use this command if you have already uploaded a file (and know its `Data Address`) and want to create a separate `PublicArchive` record for it.

```bash
gems archive <data_address_hex> [OPTIONS]
# Example using the DataAddress from a previous upload:
gems archive 7ca61972d90c00d5e2ec085c24a6c09d11eb602c27637490ec6fc9b7f7cc7351
```

*   `<data_address_hex>`: The hex string of the `Data Address` for the content you want to archive.

**Optional Path in Archive:**

You can specify the name/path this file should have *within* the archive using `-p` or `--archive-path`. This can be just a filename (like `my_video.mp4`) or include directories (like `movies/my_video.mp4`).

```bash
# Archive with just a filename:
gems archive 7ca6... --archive-path my_important_video.mp4 
# Archive with a relative path:
gems archive 7ca6... --archive-path "movies/action/my_video.mp4"
# If not specified, it defaults to "archived_file"
```

*   This command creates and uploads a *new* `PublicArchive` containing just this one entry and prints the **`Archive Address`** of the new archive.

### 3. Downloading Data (`download`)

This command can download either a single file (using its `DataAddress`) or all files within an archive (using the archive's `ArchiveAddress`).

**A) Download a Single File:**

Provide the `DataAddress` and the desired local output file path.

```bash
gems download <data_address_hex> --output-path <local/save/path/file.ext>
# Example:
gems download 7ca6... -o ./downloaded_drama.mp4
```

*   `<data_address_hex>`: The hex string of the `Data Address` of the file content.
*   `--output-path` (`-o`): The full path where the downloaded file should be saved.

**B) Download Archive Contents:**

Provide the `ArchiveAddress` and an *output directory*. Add the `--archive` flag.

```bash
gems download <archive_address_hex> --output-path <local/save/directory> --archive
# Example:
gems download <archive_address_hex> -o ./my_downloaded_archive --archive
```

*   `<archive_address_hex>`: The hex string of the `Archive Address` itself.
*   `--output-path` (`-o`): The *directory* where the archive contents should be saved. Any directory structure specified in the archive's paths (see `archive` command above) will be recreated within this output directory.
*   `--archive`: This flag tells the command to treat the address as an archive and download its contents.

## Important Notes

*   **Costs:** Uploading data and creating archives costs AttoTokens and ETH. Ensure your wallet has funds.
*   **Private Key Security:** Protect your `.env` file. Losing your private key means losing access to your wallet and potentially control over mutable data like Registers (though we aren't using them directly for archiving currently).
*   **Large Files:** Uploading very large files can take time and may be more prone to intermittent network issues. The retry mechanism helps mitigate this.
*   **Archiving:** 
    *   Creating an archive (`PublicArchive`) primarily stores metadata (paths -> data addresses). It does not duplicate the file data itself.
    *   When using the *separate* `archive` command, you can specify full relative paths for entries.
    *   Note: When choosing to archive *during the `upload` command*, the tool currently uses only the base filename from the original path for the entry within the new archive.

## License

(You should add license information here, e.g., MIT, GPL, etc.) 
