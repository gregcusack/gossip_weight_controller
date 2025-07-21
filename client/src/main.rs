use all2all_controller::instruction;
use clap::Parser;
use serde::Serialize;
use solana_client::rpc_client::RpcClient;
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
    test_interval_slots: u8,
    verify_signatures: bool,
    packet_extra_size: u16, // packet size above header size
    _future_use: [u8; 16],
}

impl TestConfig {
    fn new(test_interval_slots: u8, verify_signatures: bool, packet_extra_size: u16) -> Self {
        Self {
            test_interval_slots,
            verify_signatures,
            packet_extra_size,
            _future_use: [0u8; 16],
        }
    }
    fn as_bytes(&self) -> [u8; 4] {
        [self.test_interval_slots, 0, 0, 0]
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
    interval: u8,
    #[arg(long, short)]
    /// Whether sigverify should be called for every packet
    verify_signatures: bool,
    #[arg(long, short, default_value_t = 128)]
    /// Extra bytes to append to every packet
    packet_extra_size: u16,
    #[arg(long, short, default_value = "http://10.138.0.7:8899")]
    /// RPC URL to send transactions through
    rpc_url: String,
    #[arg(long, short, default_value = "id.json")]
    /// Payer keypair that will pay for deployment
    payer_keypair: String,
    #[arg(long, short, default_value = "all2all.json")]
    /// Keypair under which the program will write data
    storage_holder_kp: String,
}

#[tokio::main]
async fn main() {
    let cli = Commandline::parse();
    let client = RpcClient::new_with_commitment(cli.rpc_url, CommitmentConfig::confirmed());

    let record_size = std::mem::size_of::<TestConfig>();
    let account_size = RECORD_META_DATA_SIZE + record_size;
    let lamports = client
        .get_minimum_balance_for_rent_exemption(account_size)
        .unwrap();

    let payer_kp = load_keypair_from_json(&cli.payer_keypair);
    let storage_holder_kp = load_keypair_from_json(&cli.storage_holder_kp);

    // create the account
    let recent_blockhash = client.get_latest_blockhash().unwrap();
    let create_account_instruction = system_instruction::create_account(
        &payer_kp.pubkey(),
        &storage_holder_kp.pubkey(),
        lamports,
        account_size as u64,
        &program_id::ID,
    );
    let mut create_account =
        Transaction::new_with_payer(&[create_account_instruction], Some(&payer_kp.pubkey()));
    create_account.sign(&[&payer_kp, &storage_holder_kp], recent_blockhash);

    match client.send_and_confirm_transaction(&create_account) {
        Ok(signature) => println!("Account created Transaction Signature: {}", signature),
        Err(err) => eprintln!("Error sending Account create transaction: {}", err),
    }

    // Create the instruction to init the account
    let instruction_init = instruction::initialize(&storage_holder_kp.pubkey(), &payer_kp.pubkey());

    let mut transaction =
        Transaction::new_with_payer(&[instruction_init], Some(&payer_kp.pubkey()));
    transaction.sign(&[&payer_kp], client.get_latest_blockhash().unwrap());

    // Send and confirm the transaction
    match client.send_and_confirm_transaction(&transaction) {
        Ok(signature) => println!("Transaction Init Signature: {}", signature),
        Err(err) => eprintln!("Error sending Init transaction: {}", err),
    }

    // send instruction to write number into account
    let initial = TestConfig::new(cli.interval, cli.verify_signatures, cli.packet_extra_size);
    let instruction_write = instruction::write(
        &storage_holder_kp.pubkey(),
        &payer_kp.pubkey(),
        0,
        &initial.as_bytes(),
    );

    let mut transaction =
        Transaction::new_with_payer(&[instruction_write], Some(&payer_kp.pubkey()));
    transaction.sign(&[&payer_kp], client.get_latest_blockhash().unwrap());

    // Send and confirm the transaction
    match client.send_and_confirm_transaction(&transaction) {
        Ok(signature) => println!("Transaction Write Signature: {}", signature),
        Err(err) => eprintln!("Error sending transaction: {}", err),
    }
}
