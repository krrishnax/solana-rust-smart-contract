use solana_program::{
    pubkey::Pubkey,
    account_info::{AccountInfo, next_account_info},
    entrypoint::ProgramResult,
    msg,
    system_instruction,
    sysvar::{rent::Rent, Sysvar, rent::ID as RENT_PROGRAM_ID},
    program::invoke_signed,
    borsh::try_from_slice_unchecked, 
    program_error::ProgramError, program_pack::IsInitialized,
    system_program::ID as SYSTEM_PROGRAM_ID
};
use spl_token::{instruction::initialize_mint, ID as TOKEN_PROGRAM_ID};

use std::convert::TryInto;
use borsh::BorshSerialize;

use crate::instruction::MovieInstruction;
use crate::state::{MovieAccountState, MovieComment, MovieCommentCounter};
use crate::error::ReviewError;

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8]
) -> ProgramResult {
    let instruction = MovieInstruction::unpack(instruction_data)?;

    match instruction {
        MovieInstruction::AddMovieReview { title, rating, description } => {
            add_movie_review(program_id, accounts, title, rating, description)
        },
        // add UpdateMovieReview to match against our new data structure
        MovieInstruction::UpdateMovieReview { title, rating, description } => {
            // make call to update function that we'll define next
            update_movie_review(program_id, accounts, title, rating, description)
        },
        MovieInstruction::AddComments { comment } => {
            add_comment(program_id, accounts, comment)
        },
        // New instruction handled here to initialize the mint account
        MovieInstruction::InitializeMint => initialize_token_mint(program_id, accounts),
    }
}

pub fn add_movie_review(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    title: String,
    rating: u8,
    description: String,
) -> ProgramResult {
    msg!("Adding movie review...");
    msg!("Title: {}", title);
    msg!("Rating: {}", rating);
    msg!("Description: {}", description);

    let account_info_iter = &mut accounts.iter();

    let initializer = next_account_info(account_info_iter)?;
    let pda_account = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;

    // New accout to store comment count
    let pda_counter = next_account_info(account_info_iter)?;

    // ensure that the initializer of a review is also a signer on the transaction.
    if !initializer.is_signer {
        msg!("Missing required signature");
        return Err(ProgramError::MissingRequiredSignature)
    }

    let (pda, bump_seed) = Pubkey::find_program_address(
        &[
            initializer.key.as_ref(),
            title.as_bytes().as_ref(),
        ], 
        program_id
    );
    // make sure the pda_account passed in by the user is the pda we expect
    if pda != *pda_account.key {
        msg!("Invalid seeds for PDA");
        return Err(ProgramError::InvalidArgument)
    }
    // making sure rating falls within the 1 to 5 scale.
    if rating > 5 || rating < 1 {
        msg!("Rating cannot be higher than 5");
        return Err(ReviewError::InvalidRating.into())
    }

    let account_len = 1000;

    if MovieAccountState::get_account_size(title.clone(), description.clone()) > account_len {
        msg!("Data length is larger than 1000 bytes");
        return Err(ReviewError::InvalidDataLength.into());
    }

    let rent = Rent::get()?;
    let rent_lamports = rent.minimum_balance(account_len);

    invoke_signed(
        &system_instruction::create_account(
            initializer.key,
            pda_account.key, 
            rent_lamports, 
            account_len.try_into().unwrap(), 
            program_id
        ), 
        &[
            initializer.clone(),
            pda_account.clone(),
            system_program.clone(),
            ], 
        &[
            &[
                initializer.key.as_ref(),
                title.as_bytes().as_ref(),
                &[bump_seed]
            ]
        ]
    )?;

    msg!("PDA created: {}", pda);

    msg!("unpacking state account");
    let mut account_data = try_from_slice_unchecked::<MovieAccountState>(
        &pda_account
        .data
        .borrow()
    ).unwrap();

    msg!("borrowed account data");

    account_data.description = MovieAccountState::DISCRIMINATOR.to_string();
    account_data.reviewer = *initializer.key;
    account_data.title = title;
    account_data.rating = rating;
    account_data.description = description;
    account_data.is_initialized = true;

    msg!("serializing account");
    account_data.serialize(
        &mut &mut pda_account
        .data
        .borrow_mut()[..]
    )?;
    msg!("state account serialized");

    msg!("Creating comment counter");
    let rent = Rent::get()?;
    let counter_rent_lamports = rent.minimum_balance(MovieCommentCounter::SIZE);

    // Deriving the address and validating that the correct seeds were passed in
    let (counter, counter_bump) = Pubkey::find_program_address(
        &[
            pda.as_ref(),
            "comment".as_ref(),
        ], 
        program_id
    );

    if counter != *pda_counter.key {
        msg!("Invalid seeds for PDA");
        return Err(ProgramError::InvalidArgument);
    }

    // Creating the comment counter account
    invoke_signed(
        &system_instruction::create_account(
            initializer.key, // Rent payer 
            pda_counter.key, // Address who we're creating the account for
            counter_rent_lamports, // Amount of rent to put into the account
            MovieCommentCounter::SIZE.try_into().unwrap(), // Size of the account
            program_id,
        ),
        &[
            // List of accounts that will be read from/written to
            initializer.clone(),
            pda_counter.clone(),
            system_program.clone(),
        ],
        &[
            &[
                pda.as_ref(),  // Seeds for the PDA
                "comment".as_ref(),  // The string "comment"
                &[counter_bump]  // PDA account
            ]
        ],
    )?;
    msg!("Comment couner created");

    // Deserialize the newly created counter account
    let mut counter_data = try_from_slice_unchecked::<MovieCommentCounter>(
        &pda_account
        .data
        .borrow()
    ).unwrap();

    msg!("checking if ther counter account is already initialized");
    if counter_data.is_initialized() {
        msg!("Account already initialized");
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    counter_data.discriminator = MovieCommentCounter::DISCRIMINATOR.to_string();
    counter_data.counter = 0;
    counter_data.is_initialized = true;
    msg!("comment count: {}", counter_data.counter);

    counter_data.serialize(
        &mut &mut pda_account
        .data
        .borrow_mut()[..]
    )?;
    msg!("Comment counter initialized");

    Ok(())
}

pub fn update_movie_review(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    _title: String,
    rating: u8,
    description: String
) -> ProgramResult {
    msg!("Updating movie review...");

    // Get Account iterator
    let account_info_iter = &mut accounts.iter();

    // Get accounts
    let initializer = next_account_info(account_info_iter)?;
    let pda_account = next_account_info(account_info_iter)?;
		
    // This is a good time to check that the pda_account.owner is the same as the program_id
    if pda_account.owner != program_id {
        return Err(ProgramError::IllegalOwner)
    }

    // check that the signer is the same as the initializer
    if !initializer.is_signer {
        msg!("Missing required signature");
        return Err(ProgramError::MissingRequiredSignature)
    }

    // unpack the data from the pda_account
    msg!("unpacking state account");
    let mut account_data = try_from_slice_unchecked::<MovieAccountState>(
        &pda_account
        .data
        .borrow()
    ).unwrap();
    msg!("borrowed account data");

    // Derive PDA and check that it matches client
    let (pda, _bump_seed) = Pubkey::find_program_address(&[initializer.key.as_ref(), account_data.title.as_bytes().as_ref(),], program_id);

    if pda != *pda_account.key {
        msg!("Invalid seeds for PDA");
        return Err(ReviewError::InvalidPDA.into())
    }

    if !account_data.is_initialized() {
        msg!("Account is not initialized");
        return Err(ReviewError::UninitializedAccount.into());
    }

    if rating > 5 || rating < 1 {
        msg!("Rating cannot be higher than 5");
        return Err(ReviewError::InvalidRating.into())
    }

    let total_len: usize = 1 + 1 + (4 + account_data.title.len()) + (4 + description.len());
    if total_len > 1000 {
        msg!("Data length is larger than 1000 bytes");
        return Err(ReviewError::InvalidDataLength.into())
    }

    // update the account info and serialize it to account
    account_data.rating = rating;
    account_data.description = description;

    account_data.serialize(
        &mut &mut pda_account
        .data
        .borrow_mut()[..]
    )?;

    Ok(())
}

pub fn add_comment(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    comment: String
) -> ProgramResult {
    msg!("Adding Comment...");
    msg!("Comment: {}", comment);

    let account_info_iter = &mut accounts.iter();

    let commenter = next_account_info(account_info_iter)?;
    let pda_review = next_account_info(account_info_iter)?;
    let pda_counter = next_account_info(account_info_iter)?;
    let pda_comment = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;

    let mut counter_data = try_from_slice_unchecked::<MovieCommentCounter>(
        &pda_counter
        .data
        .borrow()
    ).unwrap();

    let account_len = MovieComment::get_account_size(comment.clone());

    let rent = Rent::get()?;
    let rent_lamports = rent.minimum_balance(account_len);

    let (pda, bump_seed) = Pubkey::find_program_address(&[pda_review.key.as_ref(), counter_data.counter.to_be_bytes().as_ref(),], program_id);
    if pda != *pda_comment.key {
        msg!("Invalid seeds for PDA");
        return Err(ReviewError::InvalidPDA.into())
    }

    invoke_signed(
        &system_instruction::create_account(
            commenter.key,
            pda_comment.key,
            rent_lamports,
            account_len.try_into().unwrap(),
            program_id,
        ),
        &[
            commenter.clone(), 
            pda_comment.clone(), 
            system_program.clone()
        ],
        &[
            &[
                pda_review.key.as_ref(), 
                counter_data.counter.to_be_bytes().as_ref(), 
                &[bump_seed]
            ]
        ],
    )?;

    msg!("Created Comment Account");

    let mut comment_data = try_from_slice_unchecked::<MovieComment>(
        &pda_comment
        .data
        .borrow()
    ).unwrap();


    msg!("checking if comment account is already initialized");
    if comment_data.is_initialized() {
        msg!("Account already initialized");
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    comment_data.discriminator = MovieComment::DISCRIMINATOR.to_string();
    comment_data.reviewer = *pda_review.key;
    comment_data.commenter = *commenter.key;
    comment_data.comment = comment;
    comment_data.is_initialized = true;

    comment_data.serialize(
        &mut &mut pda_comment
        .data
        .borrow_mut()[..]
    )?;

    msg!("Comment Count: {}", counter_data.counter);
    counter_data.counter += 1;
    counter_data.serialize(
        &mut &mut pda_counter
        .data
        .borrow_mut()[..]
    )?;

    Ok(())
}


// At a high level, next steps are:
// 1. Iterate through list of accounts to extract them
// 2. Derive token mint PDA
// 3. Validate all of the important accounts passed in:
//      1. Token mint account
//      2. Mint authority account
//      3. System program
//      4. Token program
//      5. Sysvar rent - the rent calculation account
// 4. Calculate rent for the mint account
// 5. Create the token mint PDA
// 6. Initialize the mint account

pub fn initialize_token_mint(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    // The order of accounts is not arbitrary, the client will send them in this order
    // Whoever sent in the transaction
    let initializer = next_account_info(account_info_iter)?;
    // Token mint PDA - derived on the client
    let token_mint = next_account_info(account_info_iter)?;
    // Token mint authority
    let mint_auth = next_account_info(account_info_iter)?;
    // System program to create a new account
    let system_program = next_account_info(account_info_iter)?;
    // Solana Token program address
    let token_program = next_account_info(account_info_iter)?;
    // System account to calcuate the rent
    let sysvar_rent = next_account_info(account_info_iter)?;

    // Derive the mint PDA again so we can validate it
    // The seed is just "token_mint"
    let (mint_pda, mint_bump) = Pubkey::find_program_address(&[b"token_mint"], program_id);
    // Derive the mint authority so we can validate it
    // The seed is just "token_auth"
    let (mint_auth_pda, _mint_auth_bump) =
        Pubkey::find_program_address(&[b"token_auth"], program_id);

    msg!("Token mint: {:?}", mint_pda);
    msg!("Mint authority: {:?}", mint_auth_pda);

    // Validate the important accounts passed in
    if mint_pda != *token_mint.key {
        msg!("Incorrect token mint account");
        return Err(ReviewError::IncorrectAccountError.into());
    }

    if *token_program.key != TOKEN_PROGRAM_ID {
        msg!("Incorrect token program");
        return Err(ReviewError::IncorrectAccountError.into());
    }

    if *mint_auth.key != mint_auth_pda {
        msg!("Incorrect mint auth account");
        return Err(ReviewError::IncorrectAccountError.into());
    }

    if *system_program.key != SYSTEM_PROGRAM_ID {
        msg!("Incorrect system program");
        return Err(ReviewError::IncorrectAccountError.into());
    }

    if *sysvar_rent.key != RENT_PROGRAM_ID {
        msg!("Incorrect rent program");
        return Err(ReviewError::IncorrectAccountError.into());
    }

    // Calculate the rent
    let rent = Rent::get()?;
    // We know the size of a mint account is 82 (remember it lol)
    let rent_lamports = rent.minimum_balance(82);

    // Create the token mint PDA
    invoke_signed(
        &system_instruction::create_account(
            initializer.key,
            token_mint.key,
            rent_lamports,
            82, // Size of the token mint account
            token_program.key,
        ),
        // Accounts we're reading from or writing to 
        &[
            initializer.clone(),
            token_mint.clone(),
            system_program.clone(),
        ],
        // Seeds for our token mint account
        &[&[b"token_mint", &[mint_bump]]],
    )?;

    msg!("Created token mint account");

    // Initialize the mint account
    invoke_signed(
        &initialize_mint(
            token_program.key,
            token_mint.key,
            mint_auth.key,
            Option::None, // Freeze authority - we don't want anyone to be able to freeze!
            9, // Number of decimals
        )?,
        // Which accounts we're reading from or writing to
        &[token_mint.clone(), sysvar_rent.clone(), mint_auth.clone()],
        // The seeds for our token mint PDA
        &[&[b"token_mint", &[mint_bump]]],
    )?;

    msg!("Initialized token mint");

    Ok(())
}