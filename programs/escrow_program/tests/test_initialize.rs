use {
    anchor_lang::{
        prelude::*,
        solana_program::{
            instruction::Instruction, program_pack::Pack, system_instruction, system_program,
        },
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        associated_token::{get_associated_token_address, spl_associated_token_account},
        token::spl_token,
    },
    escrow_program::{Escrow, ESCROW_SEED},
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

fn send(svm: &mut LiteSVM, payer: &Keypair, signers: &[&Keypair], ixs: &[Instruction]) {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx).unwrap_or_else(|err| {
        panic!("tx failed: {err:?}\nlogs: {logs:#?}", logs = err.meta.logs);
    });
}

fn create_mint(svm: &mut LiteSVM, payer: &Keypair, authority: &Pubkey) -> Keypair {
    let mint = Keypair::new();
    let rent = svm.minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN);

    let create_account_ix = system_instruction::create_account(
        &payer.pubkey(),
        &mint.pubkey(),
        rent,
        spl_token::state::Mint::LEN as u64,
        &spl_token::ID,
    );
    let init_mint_ix =
        spl_token::instruction::initialize_mint(&spl_token::ID, &mint.pubkey(), authority, None, 6)
            .unwrap();

    send(
        svm,
        payer,
        &[payer, &mint],
        &[create_account_ix, init_mint_ix],
    );
    mint
}

fn create_ata_and_mint(
    svm: &mut LiteSVM,
    payer: &Keypair,
    mint: &Pubkey,
    owner: &Pubkey,
    authority: &Keypair,
    amount: u64,
) -> Pubkey {
    let ata = get_associated_token_address(owner, mint);
    let create_ata_ix = spl_associated_token_account::instruction::create_associated_token_account(
        &payer.pubkey(),
        owner,
        mint,
        &spl_token::ID,
    );
    let mint_to_ix = spl_token::instruction::mint_to(
        &spl_token::ID,
        mint,
        &ata,
        &authority.pubkey(),
        &[],
        amount,
    )
    .unwrap();

    send(
        svm,
        payer,
        &[payer, authority],
        &[create_ata_ix, mint_to_ix],
    );
    ata
}

fn get_token_balance(svm: &LiteSVM, ata: &Pubkey) -> u64 {
    let account = svm.get_account(ata).expect("token account not found");
    let token_account =
        spl_token::state::Account::unpack(&account.data).expect("failed to unpack token account");
    token_account.amount
}

#[test]
fn test_make_update_take() {
    let program_id = escrow_program::id();
    let mut svm = LiteSVM::new();

    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/escrow_program.so"
    ));
    svm.add_program(program_id, bytes).unwrap();

    let maker = Keypair::new();
    let taker = Keypair::new();
    let mint_authority = Keypair::new();

    svm.airdrop(&maker.pubkey(), 2_000_000_000).unwrap();
    svm.airdrop(&taker.pubkey(), 2_000_000_000).unwrap();

    let mint_a = create_mint(&mut svm, &maker, &mint_authority.pubkey());
    let mint_b = create_mint(&mut svm, &maker, &mint_authority.pubkey());

    let deposit_amount = 100_000_000;
    let initial_receive_amount = 50_000_000;
    let updated_receive_amount = 60_000_000;
    let seed = 42u64;

    let maker_ata_a = create_ata_and_mint(
        &mut svm,
        &maker,
        &mint_a.pubkey(),
        &maker.pubkey(),
        &mint_authority,
        deposit_amount,
    );
    let maker_ata_b = create_ata_and_mint(
        &mut svm,
        &maker,
        &mint_b.pubkey(),
        &maker.pubkey(),
        &mint_authority,
        0,
    );
    let taker_ata_a = create_ata_and_mint(
        &mut svm,
        &taker,
        &mint_a.pubkey(),
        &taker.pubkey(),
        &mint_authority,
        0,
    );
    let taker_ata_b = create_ata_and_mint(
        &mut svm,
        &taker,
        &mint_b.pubkey(),
        &taker.pubkey(),
        &mint_authority,
        updated_receive_amount,
    );

    let (escrow_pda, _bump) = Pubkey::find_program_address(
        &[
            ESCROW_SEED,
            maker.pubkey().as_ref(),
            seed.to_le_bytes().as_ref(),
        ],
        &program_id,
    );
    let vault = get_associated_token_address(&escrow_pda, &mint_a.pubkey());

    // 1. Make
    send(
        &mut svm,
        &maker,
        &[&maker],
        &[Instruction::new_with_bytes(
            program_id,
            &escrow_program::instruction::Make {
                seed,
                deposit: deposit_amount,
                receive: initial_receive_amount,
            }
            .data(),
            escrow_program::accounts::Make {
                maker: maker.pubkey(),
                taker: taker.pubkey(),
                mint_a: mint_a.pubkey(),
                mint_b: mint_b.pubkey(),
                maker_ata_a,
                escrow: escrow_pda,
                vault,
                associated_token_program: anchor_spl::associated_token::ID,
                token_program: spl_token::ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )],
    );

    let escrow_account_data = svm.get_account(&escrow_pda).expect("escrow should exist");
    let escrow_state = Escrow::try_deserialize(&mut escrow_account_data.data.as_ref()).unwrap();
    assert_eq!(escrow_state.deposit, deposit_amount);
    assert_eq!(escrow_state.receive, initial_receive_amount);
    assert_eq!(get_token_balance(&svm, &vault), deposit_amount);
    assert_eq!(get_token_balance(&svm, &maker_ata_a), 0);

    // 2. Update
    send(
        &mut svm,
        &maker,
        &[&maker],
        &[Instruction::new_with_bytes(
            program_id,
            &escrow_program::instruction::Update {
                receive: updated_receive_amount,
            }
            .data(),
            escrow_program::accounts::Update {
                maker: maker.pubkey(),
                escrow: escrow_pda,
            }
            .to_account_metas(None),
        )],
    );

    let escrow_account_data = svm.get_account(&escrow_pda).unwrap();
    let escrow_state = Escrow::try_deserialize(&mut escrow_account_data.data.as_ref()).unwrap();
    assert_eq!(escrow_state.receive, updated_receive_amount);

    // 3. Take
    send(
        &mut svm,
        &taker,
        &[&taker],
        &[Instruction::new_with_bytes(
            program_id,
            &escrow_program::instruction::Take {}.data(),
            escrow_program::accounts::Take {
                maker: maker.pubkey(),
                taker: taker.pubkey(),
                escrow: escrow_pda,
                mint_a: mint_a.pubkey(),
                mint_b: mint_b.pubkey(),
                vault,
                taker_ata_a,
                taker_ata_b,
                maker_ata_b,
                token_program: spl_token::ID,
            }
            .to_account_metas(None),
        )],
    );

    assert!(
        svm.get_account(&escrow_pda).is_none(),
        "escrow should be closed"
    );
    assert!(svm.get_account(&vault).is_none(), "vault should be closed");
    assert_eq!(get_token_balance(&svm, &taker_ata_a), deposit_amount);
    assert_eq!(
        get_token_balance(&svm, &maker_ata_b),
        updated_receive_amount
    );
}

#[test]
fn test_make_and_refund() {
    let program_id = escrow_program::id();
    let mut svm = LiteSVM::new();

    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/escrow_program.so"
    ));
    svm.add_program(program_id, bytes).unwrap();

    let maker = Keypair::new();
    let taker = Keypair::new();
    let mint_authority = Keypair::new();

    svm.airdrop(&maker.pubkey(), 2_000_000_000).unwrap();

    let mint_a = create_mint(&mut svm, &maker, &mint_authority.pubkey());
    let mint_b = create_mint(&mut svm, &maker, &mint_authority.pubkey());

    let deposit_amount = 100_000_000;
    let receive_amount = 50_000_000;
    let seed = 99u64;

    let maker_ata_a = create_ata_and_mint(
        &mut svm,
        &maker,
        &mint_a.pubkey(),
        &maker.pubkey(),
        &mint_authority,
        deposit_amount,
    );

    let (escrow_pda, _bump) = Pubkey::find_program_address(
        &[
            ESCROW_SEED,
            maker.pubkey().as_ref(),
            seed.to_le_bytes().as_ref(),
        ],
        &program_id,
    );
    let vault = get_associated_token_address(&escrow_pda, &mint_a.pubkey());

    // 1. Make
    send(
        &mut svm,
        &maker,
        &[&maker],
        &[Instruction::new_with_bytes(
            program_id,
            &escrow_program::instruction::Make {
                seed,
                deposit: deposit_amount,
                receive: receive_amount,
            }
            .data(),
            escrow_program::accounts::Make {
                maker: maker.pubkey(),
                taker: taker.pubkey(),
                mint_a: mint_a.pubkey(),
                mint_b: mint_b.pubkey(),
                maker_ata_a,
                escrow: escrow_pda,
                vault,
                associated_token_program: anchor_spl::associated_token::ID,
                token_program: spl_token::ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )],
    );

    assert_eq!(get_token_balance(&svm, &vault), deposit_amount);
    assert_eq!(get_token_balance(&svm, &maker_ata_a), 0);

    // 4. Refund
    send(
        &mut svm,
        &maker,
        &[&maker],
        &[Instruction::new_with_bytes(
            program_id,
            &escrow_program::instruction::Refund {}.data(),
            escrow_program::accounts::Refund {
                maker: maker.pubkey(),
                escrow: escrow_pda,
                mint_a: mint_a.pubkey(),
                vault,
                maker_ata_a,
                token_program: spl_token::ID,
            }
            .to_account_metas(None),
        )],
    );

    assert!(
        svm.get_account(&escrow_pda).is_none(),
        "escrow should be closed"
    );
    assert!(svm.get_account(&vault).is_none(), "vault should be closed");
    assert_eq!(
        get_token_balance(&svm, &maker_ata_a),
        deposit_amount,
        "maker should be refunded"
    );
}