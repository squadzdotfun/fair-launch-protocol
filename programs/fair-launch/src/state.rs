use anchor_lang::prelude::*;

use crate::constants::*;

pub const STATE_OPEN: u8 = 0;
pub const STATE_LAUNCHED: u8 = 1;
pub const STATE_CANCELLED: u8 = 2;

pub const FLAG_CLAIMED: u8 = 1 << 0;
pub const FLAG_REFUNDED: u8 = 1 << 1;
/// A spot check was already tipped for catching this member below the hold line.
pub const FLAG_SPOT_TIPPED: u8 = 1 << 2;

/// Launch-level flag: the graduation-shield rent was paid back to the creator.
pub const LF_SHIELD_REPAID: u8 = 1 << 0;
/// Launch-level flag (fix #18, 22 Oct): approval-only lobby, paid for at creation. The gatekeeper only signs join
/// passes for people the creator approved; the flag makes the setting public and its fee enforceable.
pub const LF_APPROVAL: u8 = 1 << 1;

/// One member: a wallet holding 1 or 2 slots (17 Oct: one wallet, up to 2 slots). 64 bytes, no padding holes.
#[zero_copy]
#[derive(Default, Debug)]
pub struct Member {
    /// SOL owed from creator fees, not yet paid.
    pub owed: u64,
    /// Optional donation paid with the slot. Held until launch; refunded if it fails.
    pub donation: u64,
    /// HODL Rewards tokens released to this member, not yet claimed.
    pub hodl_owed: u64,
    pub wallet: Pubkey,
    /// Lowest share of their original tokens ever seen held, in bps. Drives
    /// the Fee Share (sliding) and HODL Rewards (pass/fail at 75%).
    pub lowest_bps: u16,
    pub flags: u8,
    /// Slots this wallet holds (1 or 2). Was padding: older records read 0, which means 1.
    pub slots: u8,
    pub _pad: [u8; 4],
}

impl Member {
    /// Slots held (0 on records made before slot counts existed = 1).
    pub fn slot_count(&self) -> u8 {
        self.slots.max(1)
    }
}

/// A launch. Zero-copy (read in place, no deserialisation cost). The account
/// also holds collected fees as lamports until they are paid out or swept.
#[account(zero_copy)]
pub struct Launch {
    // --- 8-byte fields ---
    pub nonce: u64,
    pub created_at: i64,
    pub period_end: i64,
    /// Lamports per slot, fixed at creation.
    pub slot_price: u64,
    pub launched_at: i64,
    /// Tokens the launch buy received (the whole curve).
    pub tokens_bought: u64,
    pub per_member_tokens: u64,
    /// SOL the buy didn't use, returned to each member at claim.
    pub leftover_per_member: u64,
    pub sol_spent: u64,
    /// Donations currently held (before launch) / paid (after launch).
    pub donations_total: u64,
    /// Creator fees waiting for the next members' round.
    pub members_pot: u64,
    /// Creator fees owed to the fee wallet.
    pub creator_owed: u64,
    pub fees_collected_total: u64,
    /// Everything this launch has owed the protocol (launch fee, its share of
    /// creator fees, pots nobody qualified for).
    pub protocol_fees_total: u64,
    pub fees_paid_total: u64,
    pub last_round_at: i64,
    /// HODL Rewards: tokens held back at launch (33% of total supply + rounding dust).
    pub hodl_reserve: u64,
    /// HODL Rewards released so far (paid out + to burn).
    pub hodl_released: u64,
    /// HODL Rewards nobody qualified for, waiting to be burned.
    pub hodl_burn_pending: u64,
    pub hodl_burned_total: u64,
    /// Chosen at launch: the coin address is derived from the launch and this number.
    pub coin_number: u64,
    /// Protocol fees held in this account until `sweep_protocol` moves them to the burn jar.
    pub protocol_owed: u64,
    /// Last time fee collection was tipped from the operations budget.
    pub last_collect_tip_at: i64,
    // --- keys ---
    pub creator: Pubkey,
    /// The coin. Unknown (all zeros) until launch.
    pub mint: Pubkey,
    /// Receives the members' optional donations at launch.
    pub donation_wallet: Pubkey,
    /// Signs join passes. All zeros = anyone can join. Fixed at creation.
    pub gatekeeper: Pubkey,
    /// Receives the creator's share of creator fees (default: the creator).
    /// Only the creator can change it, and only before launch.
    pub fee_wallet: Pubkey,
    // --- 2-byte fields ---
    /// Creator's share of creator fees after the protocol's 10%. Members get the rest.
    pub creator_fee_bps: u16,
    /// Project Treasury size in bps of total supply (0 = none). Was padding, so older accounts read 0.
    pub treasury_bps: u16,
    // --- 1-byte fields ---
    pub state: u8,
    pub slots: u8,
    pub filled: u8,
    pub bump: u8,
    pub vault_bump: u8,
    pub creator_bump: u8,
    pub mint_bump: u8,
    pub ops_bump: u8,
    pub name_len: u8,
    pub symbol_len: u8,
    pub uri_len: u8,
    /// HODL Rewards drops released so far (0-5).
    pub hodl_drops_done: u8,
    /// Required ending of the coin address, as base58 text. All zeros = none.
    pub ending: [u8; ENDING_LEN],
    /// Launch-level flags (LF_*). Was padding, so older accounts read 0.
    pub launch_flags: u8,
    /// Most slots one wallet may hold here (1 or 2), set by the creator. Was padding: 0 means 1.
    pub max_per_wallet: u8,
    pub _pad8: [u8; 2],
    // --- metadata (public and fixed from creation) ---
    pub name: [u8; 32],
    pub symbol: [u8; 16],
    pub uri: [u8; 200],
    pub members: [Member; 50],
}

impl Launch {
    pub const SPACE: usize = 8 + std::mem::size_of::<Launch>();

    pub fn raise(&self) -> u64 {
        self.slot_price * self.slots as u64
    }

    pub fn member_index(&self, wallet: &Pubkey) -> Option<usize> {
        self.members[..self.filled as usize].iter().position(|m| m.wallet == *wallet)
    }

    /// Slots taken (a wallet can hold 2). `filled` counts member records, not slots.
    pub fn slots_taken(&self) -> u8 {
        self.members[..self.filled as usize].iter().map(|m| m.slot_count()).sum()
    }

    /// Most slots one wallet may hold in this launch (0 on older launches = 1).
    pub fn per_wallet_cap(&self) -> u8 {
        self.max_per_wallet.max(1)
    }

    /// Approval-only (LF_APPROVAL), chosen and paid for at creation.
    pub fn approval_only(&self) -> bool {
        self.launch_flags & LF_APPROVAL != 0
    }

    pub fn name(&self) -> String {
        String::from_utf8_lossy(&self.name[..self.name_len as usize]).into_owned()
    }
    pub fn symbol(&self) -> String {
        String::from_utf8_lossy(&self.symbol[..self.symbol_len as usize]).into_owned()
    }
    pub fn uri(&self) -> String {
        String::from_utf8_lossy(&self.uri[..self.uri_len as usize]).into_owned()
    }

    /// Refunds are open when the creator cancelled, or the period ended unlaunched.
    pub fn refunds_open(&self, now: i64) -> bool {
        self.state == STATE_CANCELLED || (self.state == STATE_OPEN && now > self.period_end)
    }

    /// Launched and HODL Rewards drops still to come: the period the
    /// operations budget pays upkeep for.
    pub fn hodl_active(&self) -> bool {
        self.state == STATE_LAUNCHED && (self.hodl_drops_done as usize) < HODL_DROP_TIMES.len()
    }

    /// The launch needs no more upkeep from its operations budget: every drop
    /// is out, or it failed and every member has been refunded.
    pub fn upkeep_done(&self, now: i64) -> bool {
        (self.state == STATE_LAUNCHED && !self.hodl_active())
            || (self.refunds_open(now) && self.members[..self.filled as usize].iter().all(|m| m.flags & FLAG_REFUNDED != 0))
    }

    pub fn has_gatekeeper(&self) -> bool {
        self.gatekeeper != Pubkey::default()
    }

    pub fn has_ending(&self) -> bool {
        self.ending != [0u8; ENDING_LEN]
    }
}

/// A lobby's Project Treasury (docs/27): created by `add_treasury` in the creation transaction. Holds the fee
/// (lamports above rent) until launch / refund, and counts the releases. The tokens stay in the lobby vault.
#[account]
pub struct TreasuryState {
    pub launch: Pubkey,
    /// The only wallet releases can go to. Fixed forever.
    pub wallet: Pubkey,
    /// The fee held here until `settle_treasury_fee` (burned at launch, refunded otherwise).
    pub fee: u64,
    /// Tokens released so far.
    pub released: u64,
    pub bps: u16,
    /// Releases done (0-4).
    pub releases: u8,
    pub fee_settled: bool,
    pub bump: u8,
}

impl TreasuryState {
    pub const SPACE: usize = 8 + 32 + 32 + 8 + 8 + 2 + 1 + 1 + 1;
}

// Layout guards: the member table and header must stay padding-free.
const _: () = assert!(std::mem::size_of::<Member>() == 64);
const _: () = assert!(std::mem::size_of::<Launch>() == 23 * 8 + 5 * 32 + 4 + 20 + 248 + 64 * 50);
