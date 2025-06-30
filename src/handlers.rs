use axum::{extract::Json, http::StatusCode, response::Json as ResponseJson};
use base64::{Engine as _, engine::general_purpose};
use solana_program::system_instruction;
use solana_sdk::signature::{Keypair, Signature, Signer};
use spl_token::instruction as token_instruction;

use crate::dtos::{
    ApiResponse, CreateTokenRequest, InstructionData, KeypairData, MintTokenRequest,
    SendSolRequest, SendTokenRequest, SignMessageData, SignMessageRequest, SolTransferData,
    TokenAccountInfo, TokenTransferData, VerifyMessageData, VerifyMessageRequest,
};
use crate::helper::{instruction_to_response, keypair_from_base58, parse_pubkey};

pub async fn generate_keypair() -> ResponseJson<ApiResponse<KeypairData>> {
    let keypair = Keypair::new();
    let pubkey = keypair.pubkey().to_string();
    let secret = bs58::encode(&keypair.to_bytes()).into_string();

    ResponseJson(ApiResponse::success(KeypairData { pubkey, secret }))
}

pub async fn sign_message(
    Json(req): Json<SignMessageRequest>,
) -> (StatusCode, ResponseJson<ApiResponse<SignMessageData>>) {
    let message = match &req.message {
        Some(val) if !val.is_empty() => val,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string())),
            );
        }
    };

    let secret = match &req.secret {
        Some(val) if !val.is_empty() => val,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string())),
            );
        }
    };

    let keypair = match keypair_from_base58(secret) {
        Ok(kp) => kp,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(err)),
            );
        }
    };

    let message_bytes = message.as_bytes();
    let signature = keypair.sign_message(message_bytes);

    (
        StatusCode::OK,
        ResponseJson(ApiResponse::success(SignMessageData {
            signature: general_purpose::STANDARD.encode(&signature.as_ref()),
            public_key: keypair.pubkey().to_string(),
            message: message.clone(),
        })),
    )
}

pub async fn verify_message(
    Json(req): Json<VerifyMessageRequest>,
) -> (StatusCode, ResponseJson<ApiResponse<VerifyMessageData>>) {
    let message = match &req.message {
        Some(val) if !val.is_empty() => val,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string())),
            );
        }
    };

    let signature_str = match &req.signature {
        Some(val) if !val.is_empty() => val,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string())),
            );
        }
    };

    let pubkey_str = match &req.pubkey {
        Some(val) if !val.is_empty() => val,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string())),
            );
        }
    };

    let pubkey = match parse_pubkey(pubkey_str) {
        Ok(key) => key,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(err)),
            );
        }
    };

    let signature_bytes = match general_purpose::STANDARD.decode(signature_str) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Invalid base64 signature".to_string())),
            );
        }
    };

    let signature = match Signature::try_from(signature_bytes.as_slice()) {
        Ok(sig) => sig,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Invalid signature format".to_string())),
            );
        }
    };

    let valid = signature.verify(&pubkey.to_bytes(), message.as_bytes());

    (
        StatusCode::OK,
        ResponseJson(ApiResponse::success(VerifyMessageData {
            valid,
            message: message.clone(),
            pubkey: pubkey_str.clone(),
        })),
    )
}

pub async fn create_token(
    Json(req): Json<CreateTokenRequest>,
) -> (StatusCode, ResponseJson<ApiResponse<InstructionData>>) {
    let mint_authority = match &req.mint_authority {
        Some(val) if !val.is_empty() => val,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string())),
            );
        }
    };

    let mint = match &req.mint {
        Some(val) if !val.is_empty() => val,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string())),
            );
        }
    };

    let decimals = match req.decimals {
        Some(val) => val,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string())),
            );
        }
    };

    let mint_authority_pubkey = match parse_pubkey(mint_authority) {
        Ok(key) => key,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(err)),
            );
        }
    };

    let mint_pubkey = match parse_pubkey(mint) {
        Ok(key) => key,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(err)),
            );
        }
    };

    let instruction = match token_instruction::initialize_mint(
        &spl_token::id(),
        &mint_pubkey,
        &mint_authority_pubkey,
        Some(&mint_authority_pubkey),
        decimals,
    ) {
        Ok(inst) => inst,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(
                    "Failed to create token instruction".to_string(),
                )),
            );
        }
    };

    (
        StatusCode::OK,
        ResponseJson(ApiResponse::success(instruction_to_response(instruction))),
    )
}

pub async fn mint_token(
    Json(req): Json<MintTokenRequest>,
) -> (StatusCode, ResponseJson<ApiResponse<InstructionData>>) {
    let mint = req.mint.as_ref().filter(|s| !s.is_empty());
    let destination = req.destination.as_ref().filter(|s| !s.is_empty());
    let authority = req.authority.as_ref().filter(|s| !s.is_empty());
    let amount = req.amount.filter(|v| *v > 0);

    if mint.is_none() || destination.is_none() || authority.is_none() || amount.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            ResponseJson(ApiResponse::error("Missing required fields".to_string())),
        );
    }

    let mint_pubkey = match parse_pubkey(mint.unwrap()) {
        Ok(k) => k,
        Err(e) => return (StatusCode::BAD_REQUEST, ResponseJson(ApiResponse::error(e))),
    };

    let destination_pubkey = match parse_pubkey(destination.unwrap()) {
        Ok(k) => k,
        Err(e) => return (StatusCode::BAD_REQUEST, ResponseJson(ApiResponse::error(e))),
    };

    let authority_pubkey = match parse_pubkey(authority.unwrap()) {
        Ok(k) => k,
        Err(e) => return (StatusCode::BAD_REQUEST, ResponseJson(ApiResponse::error(e))),
    };

    let instruction = match token_instruction::mint_to(
        &spl_token::id(),
        &mint_pubkey,
        &destination_pubkey,
        &authority_pubkey,
        &[],
        amount.unwrap(),
    ) {
        Ok(inst) => inst,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(
                    "Failed to create mint instruction".to_string(),
                )),
            );
        }
    };

    (
        StatusCode::OK,
        ResponseJson(ApiResponse::success(instruction_to_response(instruction))),
    )
}

pub async fn send_sol(
    Json(req): Json<SendSolRequest>,
) -> (StatusCode, ResponseJson<ApiResponse<SolTransferData>>) {
    let from = req.from.as_ref().filter(|s| !s.is_empty());
    let to = req.to.as_ref().filter(|s| !s.is_empty());
    let lamports = req.lamports.filter(|v| *v > 0);

    if from.is_none() || to.is_none() || lamports.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            ResponseJson(ApiResponse::error("Missing required fields".to_string())),
        );
    }

    let from_pubkey = match parse_pubkey(from.unwrap()) {
        Ok(k) => k,
        Err(e) => return (StatusCode::BAD_REQUEST, ResponseJson(ApiResponse::error(e))),
    };

    let to_pubkey = match parse_pubkey(to.unwrap()) {
        Ok(k) => k,
        Err(e) => return (StatusCode::BAD_REQUEST, ResponseJson(ApiResponse::error(e))),
    };

    let instruction = system_instruction::transfer(&from_pubkey, &to_pubkey, lamports.unwrap());

    let response_data = SolTransferData {
        program_id: instruction.program_id.to_string(),
        accounts: instruction
            .accounts
            .iter()
            .map(|a| a.pubkey.to_string())
            .collect(),
        instruction_data: general_purpose::STANDARD.encode(&instruction.data),
    };

    (
        StatusCode::OK,
        ResponseJson(ApiResponse::success(response_data)),
    )
}

pub async fn send_token(
    Json(req): Json<SendTokenRequest>,
) -> (StatusCode, ResponseJson<ApiResponse<TokenTransferData>>) {
    // Validate required fields
    let destination = match &req.destination {
        Some(val) if !val.is_empty() => val,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string())),
            );
        }
    };

    let mint = match &req.mint {
        Some(val) if !val.is_empty() => val,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string())),
            );
        }
    };

    let owner = match &req.owner {
        Some(val) if !val.is_empty() => val,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string())),
            );
        }
    };

    let amount = match req.amount {
        Some(val) if val > 0 => val,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error("Missing required fields".to_string())),
            );
        }
    };

    let mint_pubkey = match parse_pubkey(mint) {
        Ok(key) => key,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(err)),
            );
        }
    };

    let owner_pubkey = match parse_pubkey(owner) {
        Ok(key) => key,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(err)),
            );
        }
    };

    let destination_pubkey = match parse_pubkey(destination) {
        Ok(key) => key,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(err)),
            );
        }
    };

    let source_ata =
        spl_associated_token_account::get_associated_token_address(&owner_pubkey, &mint_pubkey);
    let dest_ata = spl_associated_token_account::get_associated_token_address(
        &destination_pubkey,
        &mint_pubkey,
    );

    let instruction = match token_instruction::transfer(
        &spl_token::id(),
        &source_ata,
        &dest_ata,
        &owner_pubkey,
        &[],
        amount,
    ) {
        Ok(inst) => inst,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                ResponseJson(ApiResponse::error(
                    "Failed to create transfer instruction".to_string(),
                )),
            );
        }
    };

    let accounts = instruction
        .accounts
        .into_iter()
        .map(|acc| TokenAccountInfo {
            pubkey: acc.pubkey.to_string(),
            is_signer: acc.is_signer,
        })
        .collect();

    let response_data = TokenTransferData {
        program_id: instruction.program_id.to_string(),
        accounts,
        instruction_data: general_purpose::STANDARD.encode(&instruction.data),
    };

    (
        StatusCode::OK,
        ResponseJson(ApiResponse::success(response_data)),
    )
}
