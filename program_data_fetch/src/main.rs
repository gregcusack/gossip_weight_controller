use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::fs::File;
use std::io::Write;
use std::str::FromStr;

fn main() {
    // Your program pubkey
    let program_pubkey =
        Pubkey::from_str("recr1L3PCGKLbckBqMNcJhuuyU1zgo8nBhfLVsJNwr5").expect("Invalid pubkey");

    // BPF upgradeable loader ID
    let bpf_upgradeable_loader = solana_sdk::bpf_loader_upgradeable::id();

    // Derive ProgramData address (it's a PDA)
    let (programdata_pubkey, _) =
        Pubkey::find_program_address(&[program_pubkey.as_ref()], &bpf_upgradeable_loader);

    println!("ProgramData address: {}", programdata_pubkey);

    // Connect to RPC
    let rpc_url = "https://api.testnet.solana.com";
    let client = RpcClient::new(rpc_url.to_string());

    // Fetch the ProgramData account
    let account = client
        .get_account(&programdata_pubkey)
        .expect("Failed to fetch ProgramData account");

    println!("Account has {} lamports", account.lamports);
    println!("Owner: {}", account.owner);

    // The data field contains:
    //   [programdata_header | ELF binary]
    // We need to skip the header to get the raw ELF binary.

    // The header is:
    //   struct ProgramData {
    //       slot: u64,
    //       upgrade_authority_address: Option<Pubkey>,
    //       program_data: Vec<u8>,
    //   }
    // The header size is fixed: 8 bytes + 1 byte (option tag) + 32 bytes (if Some)
    let data = &account.data;

    if data.len() <= 41 {
        panic!("Account data too small to contain program");
    }

    // Skip header (slot + upgrade_authority option + pubkey + Vec length)
    let header_len = 8 + 1 + 32 + 4;
    let elf_data = &data[header_len..];

    // Save to file
    let mut file = File::create("program.so").expect("Failed to create file");
    file.write_all(elf_data).expect("Failed to write file");

    println!("Saved ELF binary to program.so ({} bytes)", elf_data.len());
}
