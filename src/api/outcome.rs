//! Operation services own progress; transports only mark actual attempts.
use std::{
    future::Future,
    sync::{Arc, Mutex},
};

use super::{http, profile::Route};
use crate::{
    error::{GbMcpError, Result},
    models::operation::{
        FieldIntent, FieldOutcome, FieldStatus, OperationOutcome, OperationTarget, OutcomeStatus,
        WriteRoute,
    },
};

#[derive(Clone)]
pub(crate) struct Operation(Arc<Mutex<Progress>>);

struct Progress {
    outcome: OperationOutcome,
    attempted: bool,
    possible_write: bool,
    in_flight: usize,
    // Keep a verified preflight no-op distinct from a post-write observation.
    attempted_fields: Vec<String>,
}

#[derive(Clone)]
pub(crate) struct Step {
    operation: Operation,
    fields: Vec<&'static str>,
}
pub(crate) struct Attempt {
    step: Step,
    rejected: bool,
}
tokio::task_local! { static ACTIVE_STEP: Step; }

impl Operation {
    pub(crate) fn new(
        name: &'static str,
        target: OperationTarget,
        route: Route,
        fields: &[(&'static str, bool)],
    ) -> Self {
        Self(Arc::new(Mutex::new(Progress {
            outcome: OperationOutcome {
                name: name.into(),
                status: OutcomeStatus::NotApplied,
                route: Some(match route {
                    Route::Rest => WriteRoute::Rest,
                    Route::Web => WriteRoute::Web,
                }),
                target: Some(target),
                fields: fields
                    .iter()
                    .map(|(field, requested)| FieldOutcome {
                        field: (*field).into(),
                        intent: if *requested {
                            FieldIntent::Requested
                        } else {
                            FieldIntent::Preserved
                        },
                        status: FieldStatus::NotAttempted,
                    })
                    .collect(),
            },
            attempted: false,
            possible_write: false,
            in_flight: 0,
            attempted_fields: Vec::new(),
        })))
    }

    pub(crate) async fn run<T>(&self, future: impl Future<Output = Result<T>>) -> Result<T> {
        // The outer MCP call retains this handle if it drops the service future.
        let _ = http::WRITE_POSSIBLE.try_with(|tracker| tracker.attach(self.clone()));
        // The default step covers single-request operations. Multi-step services
        // supply literal field names with step() instead.
        let result = ACTIVE_STEP
            .scope(
                Step {
                    operation: self.clone(),
                    fields: Vec::new(),
                },
                future,
            )
            .await;
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                let attempted = self.0.lock().expect("operation progress").attempted;
                if attempted {
                    Err(self.failure(error))
                } else {
                    Err(error)
                }
            }
        }
    }

    pub(crate) async fn step<T>(
        &self,
        fields: &[&'static str],
        future: impl Future<Output = Result<T>>,
    ) -> Result<T> {
        ACTIVE_STEP
            .scope(
                Step {
                    operation: self.clone(),
                    fields: fields.to_vec(),
                },
                future,
            )
            .await
    }

    pub(crate) fn verify(&self, checks: &[(&str, bool)]) -> Result<()> {
        let mismatch =
            {
                let mut progress = self.0.lock().expect("operation progress");
                let mut mismatch = false;
                for (name, matched) in checks {
                    let attempted = progress.attempted_fields.iter().any(|f| f == name);
                    if let Some(field) = progress
                        .outcome
                        .fields
                        .iter_mut()
                        .find(|field| field.field == *name)
                    {
                        field.status = if !matched {
                            FieldStatus::Mismatched
                        } else if attempted {
                            FieldStatus::Verified
                        } else {
                            FieldStatus::Unchanged
                        };
                    }
                    mismatch |= !matched;
                }
                if progress.outcome.fields.iter().all(|field| {
                    matches!(field.status, FieldStatus::Verified | FieldStatus::Unchanged)
                }) {
                    progress.outcome.status = OutcomeStatus::Verified;
                }
                mismatch
            };
        if mismatch {
            Err(self.failure(GbMcpError::Other(
                "Observed fields did not match the operation contract".into(),
            )))
        } else {
            Ok(())
        }
    }

    pub(crate) fn identity(&self, actual: u64) -> Result<()> {
        let mut progress = self.0.lock().expect("operation progress");
        let target = progress.outcome.target.as_mut().expect("service target");
        if actual == 0 || target.number.is_some_and(|number| number != actual) {
            target.observed_number = Some(actual);
            for field in &mut progress.outcome.fields {
                if field.status != FieldStatus::NotAttempted {
                    field.status = FieldStatus::Unknown;
                }
            }
            let attempted = progress.attempted;
            drop(progress);
            let error = GbMcpError::Other(
                "GitBucket returned a different or invalid resource identity".into(),
            );
            return Err(if attempted {
                self.failure(error)
            } else {
                error
            });
        }
        target.number = Some(actual);
        Ok(())
    }

    pub(crate) fn candidates(&self, numbers: Vec<u64>) {
        self.0
            .lock()
            .expect("operation progress")
            .outcome
            .target
            .as_mut()
            .expect("service target")
            .candidate_numbers = numbers;
    }

    pub(crate) fn failure(&self, error: GbMcpError) -> GbMcpError {
        let cause = match error {
            GbMcpError::OperationFailed { cause, .. } => cause,
            error => Box::new(error),
        };
        let progress = self.0.lock().expect("operation progress");
        let mut outcome = progress.outcome.clone();
        let possible = progress.possible_write || progress.in_flight != 0;
        outcome.status = if progress.attempted && !possible {
            OutcomeStatus::NotApplied
        } else {
            failure_status(&outcome.fields)
        };
        GbMcpError::OperationFailed {
            outcome: Box::new(outcome),
            cause,
        }
    }
}

fn failure_status(fields: &[FieldOutcome]) -> OutcomeStatus {
    let any = |status| fields.iter().any(|field| field.status == status);
    let incomplete = fields
        .iter()
        .any(|field| !matches!(field.status, FieldStatus::Verified | FieldStatus::Unchanged));
    if fields.iter().any(|field| {
        field.status == FieldStatus::Verified && field.intent == FieldIntent::Requested
    }) && incomplete
    {
        OutcomeStatus::Partial
    } else if !any(FieldStatus::Unknown) && any(FieldStatus::Mismatched) {
        OutcomeStatus::NotVerified
    } else {
        OutcomeStatus::Unknown
    }
}

/// Called at the transport mutation boundary, after request/auth preparation.
pub(crate) fn attempt() -> Option<Attempt> {
    ACTIVE_STEP
        .try_with(|step| {
            let mut progress = step.operation.0.lock().expect("operation progress");
            progress.attempted = true;
            progress.in_flight += 1;
            let names: Vec<_> = progress
                .outcome
                .fields
                .iter()
                .filter(|field| {
                    step.fields.is_empty() || step.fields.contains(&field.field.as_str())
                })
                .map(|field| field.field.clone())
                .collect();
            for name in &names {
                if !progress.attempted_fields.contains(name) {
                    progress.attempted_fields.push(name.clone());
                }
            }
            for field in &mut progress.outcome.fields {
                if names.contains(&field.field) {
                    field.status = FieldStatus::Unknown;
                }
            }
            Attempt {
                step: step.clone(),
                rejected: false,
            }
        })
        .ok()
}

impl Attempt {
    pub(crate) fn rejected(&mut self) {
        self.rejected = true;
        let mut progress = self.step.operation.0.lock().expect("operation progress");
        for field in &mut progress.outcome.fields {
            if self.step.fields.is_empty() || self.step.fields.contains(&field.field.as_str()) {
                field.status = FieldStatus::Rejected;
            }
        }
    }
}

impl Drop for Attempt {
    fn drop(&mut self) {
        let mut progress = self.step.operation.0.lock().expect("operation progress");
        if !self.rejected {
            progress.possible_write = true;
        }
        progress.in_flight -= 1;
    }
}

/// Low-level/custom writes lack service-owned locators and field evidence.
pub(crate) fn unknown(name: &'static str, error: GbMcpError) -> GbMcpError {
    if matches!(error, GbMcpError::OperationFailed { .. }) {
        return error;
    }
    GbMcpError::OperationFailed {
        outcome: Box::new(OperationOutcome {
            name: name.into(),
            status: OutcomeStatus::Unknown,
            route: None,
            target: None,
            fields: Vec::new(),
        }),
        cause: Box::new(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::operation::ResourceKind;

    fn operation(fields: &[(&'static str, bool)]) -> Operation {
        Operation::new(
            "test_write",
            OperationTarget::new(ResourceKind::Issue, "owner", "repo").numbered(1),
            Route::Rest,
            fields,
        )
    }
    fn forbidden() -> GbMcpError {
        GbMcpError::Api {
            status: 403,
            message: "Forbidden".into(),
        }
    }

    #[tokio::test]
    async fn fieldless_writes_distinguish_write_rejection_from_failed_verification() {
        for applied in [true, false] {
            let operation = operation(&[]);
            let error = operation
                .run(async {
                    http::mutation("write", async {
                        if applied {
                            Ok(())
                        } else {
                            Err(forbidden())
                        }
                    })
                    .await?;
                    Err::<(), _>(forbidden()) // follow-up read, after an accepted mutation
                })
                .await
                .unwrap_err();
            let GbMcpError::OperationFailed { outcome, .. } = error else {
                panic!("missing outcome");
            };
            assert_eq!(
                outcome.status,
                if applied {
                    OutcomeStatus::Unknown
                } else {
                    OutcomeStatus::NotApplied
                }
            );
        }
    }

    #[tokio::test]
    async fn rejecting_a_later_step_keeps_earlier_verified_fields() {
        let operation = operation(&[("title", true), ("body", true), ("state", true)]);
        let error = operation
            .run(async {
                operation
                    .step(&["title"], http::mutation("title", async { Ok(()) }))
                    .await?;
                operation.verify(&[("title", true)])?;
                operation
                    .step(
                        &["body"],
                        http::mutation::<()>("body", async { Err(forbidden()) }),
                    )
                    .await
            })
            .await
            .unwrap_err();
        let GbMcpError::OperationFailed { outcome, cause } = error else {
            panic!("missing outcome");
        };
        assert_eq!(outcome.status, OutcomeStatus::Partial);
        assert_eq!(
            outcome.fields.iter().map(|f| f.status).collect::<Vec<_>>(),
            [
                FieldStatus::Verified,
                FieldStatus::Rejected,
                FieldStatus::NotAttempted
            ]
        );
        assert!(matches!(*cause, GbMcpError::Api { status: 403, .. }));
    }
}
