use crate::error::Error;

/// Upper bound for `fee_bps` (5%).
pub const MAX_FEE_BPS: u32 = 500;

/// Basis-point denominator.
pub const BPS_DENOMINATOR: i128 = 10_000;

/// Splits `amount` into `(merchant_amount, fee)` using floor division on the
/// fee, so `merchant_amount + fee == amount` always holds and rounding favours
/// the merchant.
pub fn split_amount(amount: i128, fee_bps: u32) -> Result<(i128, i128), Error> {
    if amount <= 0 {
        return Err(Error::InvalidAmount);
    }
    if fee_bps > MAX_FEE_BPS {
        return Err(Error::FeeTooHigh);
    }
    let fee = amount
        .checked_mul(i128::from(fee_bps))
        .ok_or(Error::Overflow)?
        .checked_div(BPS_DENOMINATOR)
        .ok_or(Error::Overflow)?;
    let merchant_amount = amount.checked_sub(fee).ok_or(Error::Overflow)?;
    Ok((merchant_amount, fee))
}
