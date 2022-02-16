use anchor_lang::prelude::*;

/// This is the main account for each staker
/// There's also an implicitly connected token account that's a PDA
/// We don't store the token balance here so that we don't have to keep
/// the two numbers in sync.
#[account]
#[derive(Default)]
pub struct StakeAccountData {
    pub custody_bump: u8,
    pub authority_bump: u8,
    pub owner: Pubkey,
    pub lock: VestingState,
    pub positions: Vec<StakeAccountPosition>,
}

#[repr(u8)]
#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone, Copy)]
pub enum VestingState {
    VESTED,
    VESTING {
        initial_balance: u64,
        cliff_date: u64,
        vesting_duration: u64,
    },
}

impl Default for VestingState {
    fn default() -> Self {
        VestingState::VESTED
    }
}

/// This represents a staking position, i.e. an amount that someone has staked to a particular (product, publisher) tuple.
/// This is one of the core pieces of our staking design, and stores all of the state related to a position
#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone, Copy)]
pub struct StakeAccountPosition {
    pub activation_epoch: u64,
    pub unlocking_start: Option<u64>,
    pub product: Pubkey,
    pub publisher: Option<Pubkey>,
    pub amount: u64,
    // TODO: Decide if we want to reserve some space here for reward tracking state
}

impl StakeAccountPosition {
    /// Managing the state of a position is tricky because we can only update the data when a user makes a transaction
    /// but many of the state transitions take effect later, e.g. at the next epoch boundary.
    /// In order to get the actual current state, we need the current epoch. This encapsulates that logic
    /// so that other parts of the code can use the actual state.
    pub fn get_current_position(
        &self,
        current_epoch: u64,
        unlocking_duration: u64,
    ) -> Result<PositionState, ProgramError> {
        if current_epoch < self.activation_epoch - 1 {
            Ok(PositionState::ILLEGAL)
        } else if current_epoch < self.activation_epoch {
            Ok(PositionState::LOCKING)
        } else {
            match self.unlocking_start {
                Some(unlocking_start) => {
                    if (self.activation_epoch <= current_epoch) && (current_epoch < unlocking_start)
                    {
                        Ok(PositionState::LOCKED)
                    } else if (unlocking_start <= current_epoch)
                        && (current_epoch < unlocking_start + unlocking_duration)
                    {
                        Ok(PositionState::UNLOCKING)
                    } else {
                        Ok(PositionState::UNLOCKED)
                    }
                }
                None => Ok(PositionState::LOCKED),
            }
        }
    }
}

/// The core states that a position can be in
#[repr(u8)]
#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone, Copy, PartialEq)]
pub enum PositionState {
    ILLEGAL,
    UNLOCKED,
    LOCKING,
    LOCKED,
    UNLOCKING,
}

impl StakeAccountData {
    pub fn get_vested_balance(
        &self,
        current_time: u64,
        account_balance: u64,
    ) -> Result<u64, ProgramError> {
        match self.lock {
            VestingState::VESTED => Ok(account_balance),
            VestingState::VESTING {
                initial_balance,
                cliff_date,
                vesting_duration,
            } => {
                if current_time < cliff_date {
                    Ok(account_balance
                        .checked_sub(account_balance - initial_balance)
                        .unwrap())
                } else {
                    let time_passed = current_time.checked_sub(cliff_date).unwrap();
                    let completion = (time_passed as f64 / vesting_duration as f64).min(1f64);

                    let locked_amount = (initial_balance as f64 * (1f64 - completion)) as u64;
                    Ok(account_balance.checked_sub(locked_amount).unwrap())
                }
            }
        }
    }
}

    // pub fn cleanup()?

    // pub fn get_unlocked()

    // pub fn get_locked()

#[cfg(test)]
pub mod tests {
    use crate::state::stake_account::{PositionState, StakeAccountPosition};
    use anchor_lang::prelude::*;

    #[test]
    fn lifecycle_lock_unlock() {
        let p = StakeAccountPosition {
            activation_epoch: 8,
            unlocking_start: Some(12),
            product: Pubkey::new_unique(),
            publisher: None,
            amount: 10,
        };
        assert_eq!(
            PositionState::ILLEGAL,
            p.get_current_position(0, 2).unwrap()
        );
        assert_eq!(
            PositionState::LOCKING,
            p.get_current_position(7, 2).unwrap()
        );
        assert_eq!(
            PositionState::LOCKED,
            p.get_current_position(8, 2).unwrap()
        );
        assert_eq!(
            PositionState::LOCKED,
            p.get_current_position(11, 2).unwrap()
        );
        assert_eq!(
            PositionState::UNLOCKING,
            p.get_current_position(13, 2).unwrap()
        );
        assert_eq!(
            PositionState::UNLOCKED,
            p.get_current_position(14, 2).unwrap()
        );
    }

    #[test]
    fn lifecycle_lock() {
        let p = StakeAccountPosition {
            activation_epoch: 8,
            unlocking_start: None,
            product: Pubkey::new_unique(),
            publisher: None,
            amount: 10,
        };
        assert_eq!(
            PositionState::ILLEGAL,
            p.get_current_position(0, 2).unwrap()
        );
        assert_eq!(
            PositionState::LOCKING,
            p.get_current_position(7, 2).unwrap()
        );
        assert_eq!(PositionState::LOCKED, p.get_current_position(8, 2).unwrap());
        assert_eq!(
            PositionState::LOCKED,
            p.get_current_position(11, 2).unwrap()
        );
        assert_eq!(
            PositionState::LOCKED,
            p.get_current_position(300, 2).unwrap()
        );
    }
}