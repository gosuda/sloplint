use serde::{Deserialize, Serialize};

use crate::finding::Severity;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodeRole {
    Library,
    Entry,
    Test,
    Support,
    Bench,
    Example,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleRegion {
    pub start_line: u32,
    pub end_line: u32,
    pub role: CodeRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shift {
    Keep,
    Lower1,
    Lower2,
    Off,
}

impl Shift {
    pub fn apply(self, severity: Severity) -> Option<Severity> {
        match self {
            Self::Keep => Some(severity),
            Self::Lower1 => severity.lowered(1),
            Self::Lower2 => severity.lowered(2),
            Self::Off => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RolePolicy {
    pub entry: Shift,
    pub test: Shift,
    pub support: Shift,
    pub bench: Shift,
    pub example: Shift,
}

impl RolePolicy {
    pub const KEEP: RolePolicy = RolePolicy {
        entry: Shift::Keep,
        test: Shift::Keep,
        support: Shift::Keep,
        bench: Shift::Keep,
        example: Shift::Keep,
    };

    pub fn shift(self, role: CodeRole) -> Shift {
        match role {
            CodeRole::Library => Shift::Keep,
            CodeRole::Entry => self.entry,
            CodeRole::Test => self.test,
            CodeRole::Support => self.support,
            CodeRole::Bench => self.bench,
            CodeRole::Example => self.example,
        }
    }

    pub fn eligible(self, file_role: CodeRole, regions: &[RoleRegion], line: u32) -> bool {
        self.shift(role_at(file_role, regions, line)) != Shift::Off
    }
}

pub fn role_at(file_role: CodeRole, regions: &[RoleRegion], line: u32) -> CodeRole {
    let mut best: Option<RoleRegion> = None;
    for region in regions {
        if line >= region.start_line
            && line <= region.end_line
            && best.is_none_or(|current| {
                region.start_line > current.start_line
                    || (region.start_line == current.start_line
                        && region.end_line <= current.end_line)
            })
        {
            best = Some(*region);
        }
    }
    best.map_or(file_role, |region| region.role)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn severity_strategy() -> impl Strategy<Value = Severity> {
        prop_oneof![
            Just(Severity::Info),
            Just(Severity::Minor),
            Just(Severity::Major),
            Just(Severity::Critical),
            Just(Severity::Blocker),
        ]
    }

    fn shift_strategy() -> impl Strategy<Value = Shift> {
        prop_oneof![
            Just(Shift::Keep),
            Just(Shift::Lower1),
            Just(Shift::Lower2),
            Just(Shift::Off),
        ]
    }

    proptest! {
        #[test]
        fn shift_never_raises(severity in severity_strategy(), shift in shift_strategy()) {
            let shifted = shift.apply(severity);
            match shift {
                Shift::Keep => prop_assert_eq!(shifted, Some(severity)),
                Shift::Off => prop_assert_eq!(shifted, None),
                Shift::Lower1 | Shift::Lower2 => {
                    if let Some(next) = shifted {
                        prop_assert!(next <= severity);
                    }
                }
            }
        }

        #[test]
        fn role_at_returns_innermost_region(
            file_role in prop_oneof![
                Just(CodeRole::Library),
                Just(CodeRole::Entry),
                Just(CodeRole::Test),
            ],
            outer in 1u32..90,
            span in 1u32..10,
            inner_offset in 1u32..5,
        ) {
            let outer_end = outer.saturating_add(span).saturating_add(5);
            let inner_start = outer.saturating_add(inner_offset);
            let inner_end = inner_start.saturating_add(2);
            let regions = [
                RoleRegion { start_line: outer, end_line: outer_end, role: CodeRole::Test },
                RoleRegion { start_line: inner_start, end_line: inner_end, role: CodeRole::Bench },
            ];
            let flipped = [regions[1], regions[0]];
            prop_assert_eq!(role_at(file_role, &regions, inner_start), CodeRole::Bench);
            prop_assert_eq!(role_at(file_role, &flipped, inner_start), CodeRole::Bench);
            prop_assert_eq!(role_at(file_role, &regions, outer), CodeRole::Test);
            prop_assert_eq!(role_at(file_role, &flipped, outer), CodeRole::Test);
            let tied = [
                RoleRegion { start_line: 10, end_line: 20, role: CodeRole::Test },
                RoleRegion { start_line: 10, end_line: 20, role: CodeRole::Bench },
            ];
            prop_assert_eq!(role_at(file_role, &tied, 15), CodeRole::Bench);
            prop_assert_eq!(role_at(file_role, &[], outer), file_role);
        }
    }

    #[test]
    fn library_role_always_keeps() {
        let policy = RolePolicy {
            entry: Shift::Off,
            test: Shift::Off,
            support: Shift::Off,
            bench: Shift::Off,
            example: Shift::Off,
        };
        assert_eq!(policy.shift(CodeRole::Library), Shift::Keep);
    }
}
