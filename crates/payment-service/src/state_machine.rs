use common::models::PaymentStatus;
use common::errors::{AppError, AppResult};



#[derive(Debug, Clone)]
pub struct PaymentStateMachine {
    current: PaymentStatus,
}

impl PaymentStateMachine {
    pub fn new(status: PaymentStatus) -> Self {
        Self { current: status }
    }

    pub fn current(&self) -> PaymentStatus {
        self.current
    }

    
    pub fn transition(&mut self, next: PaymentStatus) -> AppResult<PaymentStatus> {
        if self.current.can_transition_to(&next) {
            let prev = self.current;
            self.current = next;
            tracing::info!(
                from = ?prev,
                to = ?next,
                "Payment state transition"
            );
            Ok(next)
        } else {
            Err(AppError::InvalidStateTransition {
                from: format!("{:?}", self.current),
                to: format!("{:?}", next),
            })
        }
    }

    pub fn is_terminal(&self) -> bool {
        self.current.is_terminal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_transitions() {
        let mut sm = PaymentStateMachine::new(PaymentStatus::Created);
        sm.transition(PaymentStatus::Pending).unwrap();
        sm.transition(PaymentStatus::Authorized).unwrap();
        sm.transition(PaymentStatus::Captured).unwrap();
        sm.transition(PaymentStatus::Settled).unwrap();
        assert!(sm.is_terminal());
    }

    #[test]
    fn test_invalid_transition_fails() {
        let mut sm = PaymentStateMachine::new(PaymentStatus::Captured);
        let result = sm.transition(PaymentStatus::Created);
        assert!(result.is_err());
    }

    #[test]
    fn test_terminal_state_no_transitions() {
        let mut sm = PaymentStateMachine::new(PaymentStatus::Failed);
        let result = sm.transition(PaymentStatus::Pending);
        assert!(result.is_err());
        assert!(sm.is_terminal());
    }

    #[test]
    fn test_cancellation_path() {
        let mut sm = PaymentStateMachine::new(PaymentStatus::Created);
        sm.transition(PaymentStatus::Cancelled).unwrap();
        assert!(sm.is_terminal());
    }

    #[test]
    fn test_refund_from_settled() {
        let mut sm = PaymentStateMachine::new(PaymentStatus::Settled);
        let result = sm.transition(PaymentStatus::Refunded);
        assert!(result.is_err());
        assert!(sm.is_terminal());
    }
}
