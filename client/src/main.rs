#![allow(clippy::arithmetic_side_effects)]
use all2all_controller::instruction;
use clap::{Parser, Subcommand};
use serde::Serialize;
use solana_client::rpc_client::RpcClient;
use solana_pubkey::Pubkey;
#[allow(deprecated)]
use solana_sdk::{
    commitment_config::CommitmentConfig,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};

const RECORD_META_DATA_SIZE: usize = 33;

#[derive(Serialize, Debug)]
#[repr(C)]
pub(crate) struct TestConfig {
    test_interval_slots: u16,
    packet_size: u16, // packet size above header size
    _future_use: [u8; 16],
}

impl TestConfig {
    fn new(test_interval_slots: u16, packet_size: u16) -> Self {
        Self {
            test_interval_slots,
            packet_size,
            _future_use: [0u8; 16],
        }
    }
}

mod program_id {
    solana_pubkey::declare_id!("recr1L3PCGKLbckBqMNcJhuuyU1zgo8nBhfLVsJNwr5");
    //solana_pubkey::declare_id!("TEsTstY62jQ8BvQkasHh1q2WvKCujNaQDgrLfZwBsiH");
}
fn load_keypair_from_json(fname: &str) -> Keypair {
    // Load keypair for the payer
    let keypair_file = std::fs::File::open(fname).unwrap();
    let payer: Vec<u8> = serde_json::from_reader(keypair_file).unwrap();
    Keypair::from_bytes(&payer).unwrap()
}

#[derive(Parser)]
struct Commandline {
    #[arg(long, short)]
    /// interval of all2all broadcasts sent out
    interval: u16,

    #[arg(long, default_value_t = 128)]
    /// Size of packets to send
    packet_size: u16,

    #[arg(long, default_value = "http://127.0.0.1:8899")]
    /// RPC URL to send transactions through
    rpc_url: String,

    #[arg(long, default_value = "id.json")]
    /// Payer keypair that will pay for deployment
    payer_keypair: String,

    #[arg(long, default_value = "all2all.json")]
    /// Keypair under which the program will write data
    account_kp: String,

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
    let client = RpcClient::new_with_commitment(cli.rpc_url, CommitmentConfig::confirmed());

    let record_size =
        bincode::serialized_size(&TestConfig::new(cli.interval, cli.packet_size)).unwrap() as usize;
    let account_size = RECORD_META_DATA_SIZE + record_size;
    let lamports = client
        .get_minimum_balance_for_rent_exemption(account_size)
        .unwrap();

    let payer_kp = load_keypair_from_json(&cli.payer_keypair);

    let storage_holder_kp = load_keypair_from_json(&cli.account_kp);

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
            let test_config = TestConfig::new(cli.interval, cli.packet_size);
            let test_config = bincode::serialize(&test_config).unwrap();
            let instruction_write = instruction::write(
                &storage_holder_kp.pubkey(),
                &payer_kp.pubkey(),
                0,
                &test_config,
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
