use crate::query::pricing;

fn invalid() {
    let _ = pricing::PricingStatus::Unpriced;
}
