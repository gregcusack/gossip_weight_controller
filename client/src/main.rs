// #![allow(clippy::arithmetic_side_effects)]
// use all2all_controller::instruction;
// use clap::{Parser, Subcommand};
// use serde::Serialize;
// use solana_client::rpc_client::RpcClient;
// use solana_pubkey::Pubkey;
// #[allow(deprecated)]
// use solana_sdk::{
//     commitment_config::CommitmentConfig,
//     signature::{Keypair, Signer},
//     system_instruction,
//     transaction::Transaction,
// };
use {
    clap::{Parser, Subcommand},
    gossip_weight_controller::instruction,
    // log::info,
    serde::Serialize,
    // solana_client::rpc_config::RpcSendTransactionConfig,
    solana_client::rpc_client::RpcClient,
    solana_commitment_config::CommitmentConfig,
    solana_keypair::read_keypair_file,
    solana_pubkey::Pubkey,
    solana_signer::Signer,
    // solana_instruction::{AccountMeta, Instruction},
    solana_system_interface::instruction as system_instruction,
    solana_transaction::Transaction,
};

const RECORD_META_DATA_SIZE: usize = 33;

#[derive(Debug, Copy, Clone, Serialize)]
#[repr(C)]
pub struct WeightingConfig {
    pub weighting_mode: u8, // 0 = Static, 1 = Dynamic
    pub tc_ms: u64,         // IIR time constant in milliseconds
    _future_use: [u8; 16],  // Reserved for future use
}

impl WeightingConfig {
    pub fn new(weighting_mode: u8, tc_ms: u64) -> Self {
        Self {
            weighting_mode,
            tc_ms,
            _future_use: [0; 16],
        }
    }

    fn as_bytes(&self) -> [u8; 9] {
        let mut bytes = [0u8; 9];
        bytes[0] = self.weighting_mode;
        bytes[1..9].copy_from_slice(&self.tc_ms.to_le_bytes());
        bytes
    }
}

mod program_id {
    // solana_program::declare_id!("5V1zhCNdTSe9Gaf38uuiJDHTpt1q6Gf3Yv7SMRk8SmwA");
    solana_pubkey::declare_id!("recr1L3PCGKLbckBqMNcJhuuyU1zgo8nBhfLVsJNwr5");
}

#[derive(Parser)]
#[command(name = "client")]
struct Commandline {
    #[arg(long, default_value = "1")]
    /// Weighting mode: 0 = Static, 1 = Dynamic
    weighting_mode: u8,

    #[arg(long, default_value = "30000")]
    /// IIR time constant in milliseconds
    tc_ms: u64,

    #[arg(long, default_value = "http://127.0.0.1:8899")]
    rpc_url: String,

    #[arg(long, default_value = "config-authority.json")]
    payer_keypair: String,

    #[arg(long, default_value = "gossip-weighting-config-account.json")]
    storage_holder_kp: String,

    #[arg(long)]
    /// Set this pubkey as authority of account. This can be e.g. multisig pubkey
    authority_pubkey: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize account state
    Init {},
    /// Write to account
    Write {},
    /// Close the account
    Close {},
}

#[tokio::main]
async fn main() {
    let cli = Commandline::parse();
    let client = RpcClient::new_with_commitment(cli.rpc_url.clone(), CommitmentConfig::confirmed());

    let payer_kp =
        read_keypair_file(&cli.payer_keypair).expect("Failed to load config account keypair");
    let storage_holder_kp =
        read_keypair_file(&cli.storage_holder_kp).expect("Failed to load storage account keypair");

    // === Create config account if needed ===
    let record_size = std::mem::size_of::<WeightingConfig>();
    let account_size = RECORD_META_DATA_SIZE + record_size;
    let lamports = client
        .get_minimum_balance_for_rent_exemption(account_size)
        .unwrap();

    // create the account
    let recent_blockhash = client.get_latest_blockhash().unwrap();

    match cli.command {
        Commands::Init {} => {
            let create_account_instruction = system_instruction::create_account(
                &payer_kp.pubkey(),
                &storage_holder_kp.pubkey(),
                lamports,
                account_size as u64,
                &program_id::ID,
            );
            let mut create_account = Transaction::new_with_payer(
                &[create_account_instruction],
                Some(&payer_kp.pubkey()),
            );

            create_account.sign(&[&payer_kp, &storage_holder_kp], recent_blockhash);
            match client.send_and_confirm_transaction(&create_account) {
                Ok(signature) => println!("Account created Transaction Signature: {}", signature),
                Err(err) => eprintln!("Error sending Account create transaction: {}", err),
            }

            let authority_pubkey = if let Some(authority_pubkey) = cli.authority_pubkey {
                Pubkey::from_str_const(&authority_pubkey)
            } else {
                payer_kp.pubkey()
            };
            // Create the instruction to init the account
            let instruction_init =
                instruction::initialize(&storage_holder_kp.pubkey(), &authority_pubkey);

            let mut transaction =
                Transaction::new_with_payer(&[instruction_init], Some(&payer_kp.pubkey()));
            transaction.sign(&[&payer_kp], client.get_latest_blockhash().unwrap());

            // Send and confirm the transaction
            match client.send_and_confirm_transaction(&transaction) {
                Ok(signature) => println!("Transaction Init Signature: {}", signature),
                Err(err) => eprintln!("Error sending Init transaction: {}", err),
            }
        }
        Commands::Write {} => {
            // send instruction to write number into account
            let initial = WeightingConfig::new(cli.weighting_mode, cli.tc_ms);
            let instruction_write = instruction::write(
                &storage_holder_kp.pubkey(),
                &payer_kp.pubkey(),
                0,
                &initial.as_bytes(),
            );
            let mut transaction =
                Transaction::new_with_payer(&[instruction_write], Some(&payer_kp.pubkey()));
            if cli.authority_pubkey.is_none() {
                transaction.sign(&[&payer_kp], client.get_latest_blockhash().unwrap());

                // Send and confirm the transaction
                match client.send_and_confirm_transaction(&transaction) {
                    Ok(signature) => println!("Transaction Write Signature: {}", signature),
                    Err(err) => eprintln!("Error sending transaction: {}", err),
                }
            } else {
                println!("Accounts: {:?}", transaction.message().account_keys);
                println!(
                    "Instruction bytes base58:\n{}\n\n",
                    bs58::encode(transaction.data(0)).into_string()
                );

                println!("Instruction bytes raw:");
                for b in transaction.data(0) {
                    print!("{b} ");
                }
                println!();
            }
        }
        Commands::Close {} => {
            let instruction_close = instruction::close_account(
                &storage_holder_kp.pubkey(),
                &payer_kp.pubkey(),
                &payer_kp.pubkey(),
            );
            let mut transaction =
                Transaction::new_with_payer(&[instruction_close], Some(&payer_kp.pubkey()));
            transaction.sign(&[&payer_kp], client.get_latest_blockhash().unwrap());

            // Send and confirm the transaction
            match client.send_and_confirm_transaction(&transaction) {
                Ok(signature) => println!("Transaction Close Signature: {}", signature),
                Err(err) => eprintln!("Error sending transaction: {}", err),
            }
        }
    }
}
