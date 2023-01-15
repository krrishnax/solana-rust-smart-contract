use solana_program::{
    pubkey::Pubkey,
    account_info::{AccountInfo, next_account_info},
    entrypoint::ProgramResult,
    msg,
    system_instruction,
    sysvar::{rent::Rent, Sysvar},
    program::invoke_signed,
    borsh::try_from_slice_unchecked, 
    program_error::ProgramError, program_pack::IsInitialized,
};

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
        }
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