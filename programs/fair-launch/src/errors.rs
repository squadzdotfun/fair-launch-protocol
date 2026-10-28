use anchor_lang::prelude::*;

#[error_code]
pub enum FlpError {
    // settings
    #[msg("Slots must be between 10 and 50")]
    BadSlots,
    #[msg("Unknown launch period")]
    BadPeriod,
    #[msg("Fee split must be within 0-100%")]
    BadFeeSplit,
    #[msg("Name, ticker or URI length out of range")]
    BadMetadata,
    #[msg("Wallet missing or not allowed")]
    BadWallets,
    #[msg("Raise too small to buy the whole pump.fun curve")]
    RaiseTooSmall,
    #[msg("Raise more than 3% above what the pump.fun curve needs")]
    RaiseTooLarge,
    #[msg("Donation must be 0 or at least 0.001 SOL")]
    BadDonation,
    #[msg("Address ending must be 4 base58 characters and not a reserved one")]
    BadEnding,
    // state machine
    #[msg("Launch is not open")]
    NotOpen,
    #[msg("Launch period has ended")]
    PeriodEnded,
    #[msg("All slots are filled")]
    Full,
    #[msg("Not all slots are filled")]
    NotFull,
    #[msg("Wallet already holds a slot")]
    AlreadyMember,
    #[msg("Wallet is not a member")]
    NotMember,
    #[msg("Launch has not happened")]
    NotLaunched,
    #[msg("Refunds are not open")]
    RefundsNotOpen,
    #[msg("Already claimed")]
    AlreadyClaimed,
    #[msg("Too early")]
    TooEarly,
    #[msg("The launch still needs upkeep")]
    UpkeepNotDone,
    // authority
    #[msg("Only the creator can do this")]
    NotCreator,
    #[msg("Join pass missing or malformed")]
    PassMissing,
    #[msg("Join pass not signed by this lobby's gatekeeper for this wallet")]
    BadPass,
    #[msg("Join pass expired")]
    PassExpired,
    #[msg("The fee wallet is locked once the launch is no longer open")]
    FeeWalletLocked,
    // accounts
    #[msg("Account does not match the launch")]
    AccountMismatch,
    #[msg("Wrong number of accounts")]
    AccountCount,
    #[msg("Coin address does not end in the launch's ending")]
    WrongEnding,
    // pump.fun results
    #[msg("Bonding curve not complete after the buy")]
    CurveNotComplete,
    #[msg("Tokens received do not match the curve")]
    WrongTokenAmount,
    #[msg("Nothing to claim")]
    NothingToClaim,
    #[msg("Arithmetic overflow")]
    Overflow,
    // one wallet, up to 2 slots (17 Oct; new codes go at the end so the old ones keep their numbers)
    #[msg("Slot count must be 1 or 2")]
    BadSlotCount,
    #[msg("This wallet can't hold that many slots in this lobby")]
    TooManySlots,
    // audit run 5 (18 Oct)
    #[msg("Slots per person can only be set when the lobby is created")]
    SlotRuleLocked,
    // Project Treasury (20 Oct)
    #[msg("Treasury must be 1-10% in whole percent, with a wallet that isn't one of this lobby's accounts")]
    BadTreasury,
    #[msg("A treasury can only be added when the lobby is created")]
    TreasuryLocked,
    #[msg("The treasury fee is already settled")]
    TreasurySettled,
    // Fixes #3 / #18 (22 Oct)
    #[msg("This setting can only be chosen when the lobby is created")]
    SettingLocked,
    #[msg("Approval-only needs a gatekeeper (the key that signs join passes)")]
    NoGatekeeper,
    // $SQUADZ on Meteora (22 Oct)
    #[msg("$SQUADZ (launch #1) launches on Meteora; every other launch on pump.fun")]
    WrongVenue,
    // audit 22 Oct
    #[msg("$SQUADZ's Meteora settings or coin aren't the decided ones")]
    WrongMeteoraSettings,
    // audit run 6 (23 Oct)
    #[msg("Pay back the graduation shield first (repay_shield), then close the operations budget")]
    ShieldNotRepaid,
}
