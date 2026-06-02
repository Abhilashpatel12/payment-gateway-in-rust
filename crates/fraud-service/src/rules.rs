









use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;


#[derive(Debug, Deserialize)]
pub struct FraudContext {
    pub payment_id: Uuid,
    pub merchant_id: Uuid,
    pub amount: i64,
    pub currency: String,
    pub card_token: Option<String>,
    pub customer_email: Option<String>,
    pub ip_address: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RuleResult {
    pub rule_name: &'static str,
    pub blocked: bool,
    pub risk_score: i32, 
    pub reason: Option<String>,
}

#[async_trait]
pub trait FraudRule: Send + Sync {
    fn name(&self) -> &'static str;
    async fn evaluate(&self, ctx: &FraudContext, db: &PgPool) -> Result<RuleResult>;
}



pub struct VelocityRule {
    
    pub max_payments: i64,
    
    pub window_minutes: i64,
}

#[async_trait]
impl FraudRule for VelocityRule {
    fn name(&self) -> &'static str {
        "velocity_check"
    }

    async fn evaluate(&self, ctx: &FraudContext, db: &PgPool) -> Result<RuleResult> {
        let Some(token) = &ctx.card_token else {
            return Ok(RuleResult {
                rule_name: self.name(),
                blocked: false,
                risk_score: 0,
                reason: None,
            });
        };

        let count: i64 = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM payments p
            WHERE p.payment_method->>'token' = $1
              AND p.created_at >= NOW() - INTERVAL '1 minute' * $2
              AND p.status NOT IN ('cancelled', 'failed')
            "#,
        )
        .bind(token)
        .bind(self.window_minutes)
        .fetch_one(db)
        .await?;

        let blocked = count >= self.max_payments;
        let risk_score = ((count as f64 / self.max_payments as f64) * 100.0).min(100.0) as i32;

        Ok(RuleResult {
            rule_name: self.name(),
            blocked,
            risk_score,
            reason: if blocked {
                Some(format!(
                    "Card token used {} times in {} minutes (max {})",
                    count, self.window_minutes, self.max_payments
                ))
            } else {
                None
            },
        })
    }
}



pub struct AmountThresholdRule {
    
    pub threshold_minor: i64,
}

#[async_trait]
impl FraudRule for AmountThresholdRule {
    fn name(&self) -> &'static str {
        "amount_threshold"
    }

    async fn evaluate(&self, ctx: &FraudContext, _db: &PgPool) -> Result<RuleResult> {
        let blocked = ctx.amount > self.threshold_minor;
        let risk_score = if blocked { 60 } else { 0 };

        Ok(RuleResult {
            rule_name: self.name(),
            blocked,
            risk_score,
            reason: if blocked {
                Some(format!(
                    "Amount {} exceeds threshold {} {}",
                    ctx.amount, self.threshold_minor, ctx.currency
                ))
            } else {
                None
            },
        })
    }
}



pub struct BlocklistRule;

#[async_trait]
impl FraudRule for BlocklistRule {
    fn name(&self) -> &'static str {
        "blocklist"
    }

    async fn evaluate(&self, ctx: &FraudContext, db: &PgPool) -> Result<RuleResult> {
        let Some(token) = &ctx.card_token else {
            return Ok(RuleResult {
                rule_name: self.name(),
                blocked: false,
                risk_score: 0,
                reason: None,
            });
        };

        
        let blocked: Option<bool> = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT true
            FROM fraud_checks fc
            WHERE fc.merchant_id = $1
              AND fc.blocked = true
              AND $2 = ANY(fc.triggered_rules)
            LIMIT 1
            "#,
        )
        .bind(ctx.merchant_id)
        .bind(token)
        .fetch_optional(db)
        .await?;

        Ok(RuleResult {
            rule_name: self.name(),
            blocked: blocked.unwrap_or(false),
            risk_score: if blocked.is_some() { 100 } else { 0 },
            reason: if blocked.is_some() {
                Some("Card token found on blocklist".to_string())
            } else {
                None
            },
        })
    }
}




pub struct FraudEngine {
    rules: Vec<Box<dyn FraudRule>>,
}

#[derive(Debug, Serialize)]
pub struct EvaluationResult {
    pub payment_id: Uuid,
    pub blocked: bool,
    pub risk_score: i32,
    pub decision: &'static str, 
    pub triggered_rules: Vec<&'static str>,
    pub reasons: Vec<String>,
}

impl FraudEngine {
    pub fn default_engine() -> Self {
        Self {
            rules: vec![
                Box::new(VelocityRule { max_payments: 5, window_minutes: 10 }),
                Box::new(AmountThresholdRule { threshold_minor: 100_000_00 }), 
                Box::new(BlocklistRule),
            ],
        }
    }

    pub async fn evaluate(&self, ctx: &FraudContext, db: &PgPool) -> Result<EvaluationResult> {
        let mut max_risk = 0i32;
        let mut blocked = false;
        let mut triggered = Vec::new();
        let mut reasons = Vec::new();

        for rule in &self.rules {
            let result = rule.evaluate(ctx, db).await?;

            if result.risk_score > max_risk {
                max_risk = result.risk_score;
            }

            if result.blocked {
                blocked = true;
                triggered.push(result.rule_name);
                if let Some(r) = result.reason {
                    reasons.push(r);
                }
                
                break;
            } else if result.risk_score >= 40 {
                triggered.push(result.rule_name);
            }
        }

        let decision = if blocked {
            "block"
        } else if max_risk >= 40 {
            "review"
        } else {
            "allow"
        };

        Ok(EvaluationResult {
            payment_id: ctx.payment_id,
            blocked,
            risk_score: max_risk,
            decision,
            triggered_rules: triggered,
            reasons,
        })
    }
}
