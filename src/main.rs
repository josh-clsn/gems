use autonomi::{Bytes, Client, Wallet, AttoTokens, XorName};
use autonomi::files::Metadata;
use autonomi::data::DataAddress;
use autonomi::client::payment::PaymentOption;
use autonomi::client::files::archive_public::{PublicArchive, ArchiveAddress};
use clap::{Parser, Subcommand};
use eyre::{Result, WrapErr, eyre};
use std::path::{Path, PathBuf};
use std::env;
use std::io::{stdin, stdout, Write};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs::{read, write, create_dir_all};
use tokio::time::{sleep, Duration};
use hex;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Upload a file, optionally verify, and optionally archive
    Upload(UploadArgs),
    /// Create a new archive containing a reference to a previously uploaded file
    Archive(ArchiveArgs),
    /// Download a file (using DataAddress) or the contents of an archive (using ArchiveAddress)
    Download(DownloadArgs),
}

#[derive(Parser, Debug)]
struct UploadArgs {
    /// Path to the file to upload
    #[arg(short, long)]
    file_path: PathBuf,

    /// Optional: Directory to download the file to during verification
    #[arg(short, long, default_value = ".")]
    output_dir: PathBuf,
}

#[derive(Parser, Debug)]
struct ArchiveArgs {
    /// The DataAddress (as hex string) of the file to create an archive for
    #[arg(index = 1)]
    data_address_hex: String,

    /// Optional: The path/name to use for the file within the new archive
    #[arg(short, long, default_value = "archived_file")]
    archive_path: String, 
}

#[derive(Parser, Debug)]
struct DownloadArgs {
    /// The DataAddress or ArchiveAddress (as hex string) to download from
    #[arg(index = 1)]
    address_hex: String,

    /// Path to save the downloaded file or directory.
    /// If --archive is used, this is the base directory.
    /// If not, this is the full file path.
    #[arg(short, long)]
    output_path: PathBuf,

    /// Treat the address as an ArchiveAddress and download all its contents
    #[arg(long)]
    archive: bool,
}

// Helper function for interactive prompts
fn ask_yes_no(prompt: &str) -> Result<bool> {
    loop {
        print!("{} [y/n]: ", prompt);
        stdout().flush().wrap_err("Failed to flush stdout")?;
        let mut input = String::new();
        stdin().read_line(&mut input).wrap_err("Failed to read line")?;
        match input.trim().to_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => println!("Invalid input. Please enter 'y' or 'n'."),
        }
    }
}

// Function to use PublicArchive and add retries
async fn perform_archive_action(
    client: &Client,
    payment: PaymentOption,
    data_addr: &DataAddress,
    original_path: &Path,
    metadata: &Metadata
) -> Result<ArchiveAddress> {
    println!("--- Performing Archive Action (using PublicArchive) ---");
    println!("Creating new archive for DataAddress: {:?} (original path: {:?})", data_addr, original_path);

    let mut archive = PublicArchive::new();
    let archive_path = original_path.file_name()
        .ok_or_else(|| eyre!("Could not get filename for archive path"))?
        .into();
    archive.add_file(archive_path, *data_addr, metadata.clone());

    println!("Attempting to upload new PublicArchive with retries (max 50 attempts)...");

    let max_retries = 50;
    let mut archive_upload_result: Option<(AttoTokens, ArchiveAddress)> = None;

    for attempt in 1..=max_retries {
        println!("  --- Archive Upload Attempt {}/{} ---", attempt, max_retries);
        // Clone payment option for each attempt inside the loop
        match client.archive_put_public(&archive, payment.clone()).await {
            Ok((cost, archive_address)) => {
                println!("  Successfully uploaded PublicArchive on attempt {}!", attempt);
                archive_upload_result = Some((cost, archive_address));
                break; // Exit loop on success
            }
            Err(e) => {
                println!(
                    "  Archive upload attempt {} failed: {}. Retrying in 5 seconds...",
                    attempt,
                    e
                );
                if attempt == max_retries {
                    // Error already includes context from archive_put_public
                    return Err(eyre::eyre!("Failed to upload archive after {} attempts: {}", max_retries, e));
                }
                sleep(Duration::from_secs(5)).await;
            }
        }
    }

    // Check if archive upload succeeded
    let (cost, archive_address) = archive_upload_result
        .ok_or_else(|| eyre!("Archive upload failed after {} attempts.", max_retries))?;

    println!("Archive Upload successful!");
    println!("  Archive Cost: {} AttoTokens", cost);
    println!("  Archive Address: {:?}", archive_address);

    println!("--- Archive Action (PublicArchive) Complete ---");
    Ok(archive_address)
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let cli = Cli::parse();

    println!("Initializing client...");
    let client = Client::init().await.wrap_err("Failed to initialize client")?;
    println!("Client initialized.");

    println!("Setting up wallet from environment variable...");
    let pk_hex = env::var("AUTONOMI_PRIVATE_KEY")
        .map_err(|_| eyre!("AUTONOMI_PRIVATE_KEY environment variable not set."))?;
    let key = pk_hex;
    let wallet = Wallet::new_from_private_key(Default::default(), &key)
        .wrap_err("Failed to create wallet from private key")?;
    let payment = PaymentOption::Wallet(wallet.clone());
    println!("Wallet setup complete using provided private key.");

    match cli.command {
        Commands::Upload(args) => {
            handle_upload(client, payment, args).await?
        }
        Commands::Archive(args) => {
            handle_archive(client, payment, args).await?
        }
        Commands::Download(args) => {
            handle_download(client, args).await?
        }
    }

    Ok(())
}

async fn handle_upload(client: Client, payment: PaymentOption, args: UploadArgs) -> Result<()> {
    // --- File Reading & Metadata ---
    println!("Reading file: {:?}...", args.file_path);
    let file_content = read(&args.file_path)
        .await
        .wrap_err_with(|| format!("Failed to read file: {:?}", args.file_path))?;
    let original_data = Bytes::from(file_content);
    let file_size = original_data.len() as u64;
    let system_time_now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs();
    let mut file_metadata = Metadata::new_with_size(file_size);
    file_metadata.created = system_time_now;
    file_metadata.modified = system_time_now;

    println!("Read {} bytes from file.", original_data.len());

    // --- Ask questions BEFORE upload --- 
    println!("\nConfiguration for after upload completes:");
    let should_verify = ask_yes_no("Download and verify the uploaded data afterwards?")?;
    let should_archive = ask_yes_no("Create a new archive for this upload afterwards?")?;

    // --- Upload Loop ---
    println!("\nAttempting to upload file with retries (max 50 attempts)...");
    let max_retries = 50;
    let mut upload_result: Option<(AttoTokens, DataAddress)> = None;

    for attempt in 1..=max_retries {
        println!("\n--- Upload Attempt {}/{} ---", attempt, max_retries);
        match client
            .data_put_public(original_data.clone(), payment.clone()) // Clone payment for loop
            .await
        {
            Ok((cost, data_addr)) => {
                println!("Upload successful on attempt {}!", attempt);
                upload_result = Some((cost, data_addr));
                break;
            }
            Err(e) => {
                println!("Upload attempt {} failed: {}. Retrying in 5 seconds...", attempt, e);
                if attempt == max_retries {
                    return Err(eyre!("Failed to upload file after {} attempts: {}", max_retries, e));
                }
                sleep(Duration::from_secs(5)).await;
            }
        }
    }

    // Check if upload succeeded
    let (cost, data_addr) = upload_result
        .ok_or_else(|| eyre!("Upload failed after {} attempts.", max_retries))?;

    // If upload succeeded, proceed based on answers given earlier
    println!("\nUpload successful!");
    println!("  Cost: {} AttoTokens", cost);
    println!("  Data Address: {:?}", data_addr);

    // --- Conditional Download/Verification (based on earlier answer) ---
    if should_verify {
        println!("\nProceeding with download and verification...");
        println!("Downloading file using data_get_public...");
        match client.data_get_public(&data_addr).await {
            Ok(fetched_data) => {
                println!("Download successful! Fetched {} bytes.", fetched_data.len());
                println!("Verifying downloaded data...");
                if original_data == fetched_data {
                    println!("Verification successful: Original and downloaded data match.");
                    let output_filename = args.output_dir.join(
                        args.file_path.file_name().ok_or_else(|| eyre!("Could not get filename"))?
                    );
                    println!("Saving verified file to {:?}", output_filename);
                    if let Err(e) = write(&output_filename, original_data).await {
                         println!("Warning: Failed to write verified file: {}", e);
                    }
                } else {
                    println!("Verification failed: Data mismatch!");
                    // Save downloaded file for inspection even on mismatch
                    let output_filename = args.output_dir.join(
                        args.file_path.file_name().ok_or_else(|| eyre!("Could not get filename"))?.to_str().unwrap_or("downloaded_file_error").to_owned() + ".mismatched"
                    );
                    println!("Saving mismatched downloaded file to {:?} for inspection.", output_filename);
                    if let Err(e) = write(&output_filename, fetched_data).await {
                        println!("Warning: Failed to write mismatched downloaded file: {}", e);
                    }
                }
            }
            Err(e) => {
                // Log error if download/get fails during verification step
                println!("Error during verification download: {}. Skipping verification.", e);
            }
        }
    } else {
        println!("Skipping download and verification.");
    }

    // --- Conditional Archive (based on earlier answer) ---
    if should_archive {
        println!("\nProceeding with archive creation...");
        match perform_archive_action(
             &client,
             payment.clone(), // Clone payment as it might have been consumed if verify ran
             &data_addr,
             &args.file_path,
             &file_metadata
         ).await {
            Ok(_new_archive_address) => {
                 // Success message is printed within perform_archive_action
            }
            Err(e) => {
                // Log error if archive action fails
                println!("Error during archive creation: {}", e);
            }
         }
    } else {
        println!("Skipping archive creation.");
    }

    println!("\nUpload process completed.");
    Ok(())
}

// Updated to create a new archive for a given DataAddress
async fn handle_archive(client: Client, payment: PaymentOption, args: ArchiveArgs) -> Result<()> {
    println!("Attempting to create new archive for data address: {}", args.data_address_hex);

    // Parse the hex string into XorName bytes
    let xorname_bytes = hex::decode(&args.data_address_hex)
        .wrap_err("Invalid hex string for DataAddress XorName")?;

    // Convert Vec<u8> to [u8; 32] for XorName construction
    let xorname_array: [u8; 32] = xorname_bytes.as_slice().try_into()
        .map_err(|_| eyre!("Hex string does not represent a valid XorName (expected 32 bytes, got {})", xorname_bytes.len()))?;
    
    // Construct XorName from the array
    let xorname = XorName(xorname_array);
    let data_addr = DataAddress::new(xorname);

    // Create basic metadata (we don't have the original file size here easily)
    let system_time_now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs();
    let mut metadata = Metadata::new_with_size(0); // Size is unknown here, set to 0
    metadata.created = system_time_now;
    metadata.modified = system_time_now;

    // Use the provided archive path
    let archive_path = PathBuf::from(&args.archive_path);

    // Call the action function to create and upload the new archive
    let new_archive_address = perform_archive_action(
        &client,
        payment,
        &data_addr,
        &archive_path, // Use the path provided/defaulted for the archive entry
        &metadata
    ).await?;

    println!("\nArchive creation process completed.");
    println!("New archive created with address: {:?}", new_archive_address);
    Ok(())
}

async fn handle_download(client: Client, args: DownloadArgs) -> Result<()> {
    println!("Attempting to download from address: {}", args.address_hex);

    // Parse the hex string into XorName bytes
    let xorname_bytes = hex::decode(&args.address_hex)
        .wrap_err("Invalid hex string for Address XorName")?;

    // Convert Vec<u8> to [u8; 32] for XorName construction
    let xorname_array: [u8; 32] = xorname_bytes.as_slice().try_into()
        .map_err(|_| eyre!("Hex string does not represent a valid XorName (expected 32 bytes, got {})", xorname_bytes.len()))?;
    
    let addr = DataAddress::new(XorName(xorname_array));

    if args.archive {
        // --- Download Archive Contents ---
        println!("Fetching archive data from {:?}...", addr);
        let fetched_archive_bytes = client.data_get_public(&addr).await
            .wrap_err_with(|| format!("Failed to get public data for archive address: {:?}", addr))?;
        
        println!("Deserializing archive data...");
        let archive = PublicArchive::from_bytes(fetched_archive_bytes)
            .wrap_err("Failed to deserialize PublicArchive data")?;
        
        if archive.map().is_empty() {
            println!("Archive is empty. Nothing to download.");
            return Ok(());
        }

        println!("Downloading archive contents to directory: {:?}", args.output_path);
        create_dir_all(&args.output_path).await
            .wrap_err_with(|| format!("Failed to create output directory: {:?}", args.output_path))?;

        let mut success_count = 0;
        let mut error_count = 0;
        
        for (item_path, item_data_addr, _metadata) in archive.iter() {
            let target_file_path = args.output_path.join(item_path);
            println!("  Downloading {:?} (from {:?}) -> {:?}", item_path, item_data_addr, target_file_path);
            
            // Ensure parent directory exists
            if let Some(parent_dir) = target_file_path.parent() {
                if !parent_dir.exists() {
                    create_dir_all(parent_dir).await
                        .wrap_err_with(|| format!("Failed to create subdirectory: {:?}", parent_dir))?;
                }
            }

            match client.data_get_public(item_data_addr).await {
                Ok(item_bytes) => {
                    match write(&target_file_path, item_bytes).await {
                        Ok(_) => {
                            println!("    Successfully saved {:?}", target_file_path);
                            success_count += 1;
                        }
                        Err(e) => {
                            println!("    Error saving file {:?}: {}", target_file_path, e);
                            error_count += 1;
                        }
                    }
                }
                Err(e) => {
                    println!("    Error downloading data for {:?} ({:?}): {}", item_path, item_data_addr, e);
                    error_count += 1;
                }
            }
        }
        println!("\nArchive download complete. {} files succeeded, {} files failed.", success_count, error_count);

    } else {
        // --- Download Single File ---
        println!("Fetching single file data from {:?}...", addr);
        let fetched_bytes = client.data_get_public(&addr).await
            .wrap_err_with(|| format!("Failed to get public data for address: {:?}", addr))?;

        println!("Saving file to: {:?}", args.output_path);
        // Ensure parent directory exists for the single file case too
        if let Some(parent_dir) = args.output_path.parent() {
             if !parent_dir.exists() {
                 create_dir_all(parent_dir).await
                     .wrap_err_with(|| format!("Failed to create output directory: {:?}", parent_dir))?;
             }
         }
        
        write(&args.output_path, fetched_bytes).await
            .wrap_err_with(|| format!("Failed to write downloaded file to: {:?}", args.output_path))?;
        println!("Successfully downloaded and saved single file.");
    }

    Ok(())
} 
