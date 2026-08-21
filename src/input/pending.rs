//! Pending preview counts and multi-key prefixes.

use std::num::NonZeroU32;

/// The highest a pending count may grow: `99999j` and `9999999999j` both
/// scroll to the document's end instead of overflowing.
const MAX_COUNT: u32 = 99_999;

/// A non-zero, saturated preview command count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct Count(NonZeroU32);

impl Count {
    fn new(value: u32) -> Option<Self> {
        NonZeroU32::new(value.min(MAX_COUNT)).map(Self)
    }

    fn push_digit(self, digit: u32) -> Self {
        let value = self
            .get()
            .saturating_mul(10)
            .saturating_add(digit)
            .min(MAX_COUNT);

        Self(NonZeroU32::new(value).expect("appending to a non-zero count stays non-zero"))
    }

    fn get(self) -> u32 {
        self.0.get()
    }
}

/// A preview command prefix awaiting its second key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Prefix {
    G,
    Z,
}

/// The mutually exclusive pending input accepted by the preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(super) enum Pending {
    #[default]
    Idle,
    Count(Count),
    Prefix {
        prefix: Prefix,
        count: Option<Count>,
    },
}

impl Pending {
    /// The count shown by the mode badge, or zero when no count is pending.
    pub(super) fn pending_count(self) -> u32 {
        self.count().map_or(0, Count::get)
    }

    /// Whether digits have already started a count.
    pub(super) fn has_count(self) -> bool {
        self.count().is_some()
    }

    /// The count a motion repeats: the typed digits, or once.
    pub(super) fn motion_count(self) -> usize {
        self.count().map_or(1, Count::get) as usize
    }

    /// The count a jump carries: the typed digits, or none. This preserves
    /// the distinct first/last meaning of a plain `gg` or `G`.
    pub(super) fn jump_count(self) -> usize {
        self.count().map_or(0, Count::get) as usize
    }

    /// Whether this pending sequence is waiting for `prefix`.
    pub(super) fn has_prefix(self, expected: Prefix) -> bool {
        matches!(self, Self::Prefix { prefix, .. } if prefix == expected)
    }

    /// Arms a prefix, replacing any other prefix while preserving its count.
    pub(super) fn arm(&mut self, prefix: Prefix) {
        *self = Self::Prefix {
            prefix,
            count: self.count(),
        };
    }

    /// Appends one digit, cancelling a prefix while preserving its count.
    /// A zero without an existing count returns to idle, so [`Count`] can
    /// never represent zero.
    pub(super) fn push_digit(&mut self, digit: u32) {
        *self = match self.count() {
            Some(count) => Self::Count(count.push_digit(digit)),
            None => Count::new(digit).map_or(Self::Idle, Self::Count),
        };
    }

    /// Consumes or cancels all pending sequence input.
    pub(super) fn consume(&mut self) {
        *self = Self::Idle;
    }

    fn count(self) -> Option<Count> {
        match self {
            Self::Idle => None,
            Self::Count(count)
            | Self::Prefix {
                count: Some(count), ..
            } => Some(count),
            Self::Prefix { count: None, .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Count, Pending, Prefix, MAX_COUNT};

    #[derive(Clone, Copy)]
    enum Operation {
        Digit(u32),
        Arm(Prefix),
        Consume,
    }

    fn apply(operations: &[Operation]) -> Pending {
        let mut pending = Pending::default();

        for operation in operations {
            match operation {
                Operation::Digit(digit) => pending.push_digit(*digit),
                Operation::Arm(prefix) => pending.arm(*prefix),
                Operation::Consume => pending.consume(),
            }
        }

        pending
    }

    fn count(value: u32) -> Count {
        Count::new(value).expect("test counts are non-zero")
    }

    #[test]
    fn sequence_operations_follow_the_transition_table() {
        use Operation::{Arm, Consume, Digit};
        use Prefix::{G, Z};

        let cases: &[(&str, &[Operation], Pending)] = &[
            ("lone zero", &[Digit(0)], Pending::Idle),
            ("single digit", &[Digit(3)], Pending::Count(count(3))),
            (
                "zero extends a count",
                &[Digit(1), Digit(0)],
                Pending::Count(count(10)),
            ),
            (
                "count saturates",
                &[Digit(9), Digit(9), Digit(9), Digit(9), Digit(9), Digit(9)],
                Pending::Count(count(MAX_COUNT)),
            ),
            (
                "g preserves count",
                &[Digit(3), Arm(G)],
                Pending::Prefix {
                    prefix: G,
                    count: Some(count(3)),
                },
            ),
            (
                "z preserves count",
                &[Digit(4), Arm(Z)],
                Pending::Prefix {
                    prefix: Z,
                    count: Some(count(4)),
                },
            ),
            (
                "g is replaced by z",
                &[Arm(G), Arm(Z)],
                Pending::Prefix {
                    prefix: Z,
                    count: None,
                },
            ),
            (
                "z is replaced by g",
                &[Arm(Z), Arm(G)],
                Pending::Prefix {
                    prefix: G,
                    count: None,
                },
            ),
            (
                "digit cancels prefix",
                &[Arm(G), Digit(3)],
                Pending::Count(count(3)),
            ),
            (
                "digit continues count after prefix",
                &[Digit(3), Arm(Z), Digit(0)],
                Pending::Count(count(30)),
            ),
            (
                "zero cancels uncounted prefix",
                &[Arm(Z), Digit(0)],
                Pending::Idle,
            ),
            (
                "consumption clears count and prefix",
                &[Digit(3), Arm(G), Consume],
                Pending::Idle,
            ),
        ];

        for (name, operations, expected) in cases {
            assert_eq!(apply(operations), *expected, "{name}");
        }
    }

    #[test]
    fn count_projections_follow_the_transition_table() {
        let cases = [
            ("idle", Pending::Idle, 0, 1, 0),
            ("count", Pending::Count(count(3)), 3, 3, 3),
            (
                "uncounted prefix",
                Pending::Prefix {
                    prefix: Prefix::G,
                    count: None,
                },
                0,
                1,
                0,
            ),
            (
                "counted prefix",
                Pending::Prefix {
                    prefix: Prefix::Z,
                    count: Some(count(5)),
                },
                5,
                5,
                5,
            ),
        ];

        for (name, pending, badge, motion, jump) in cases {
            assert_eq!(pending.pending_count(), badge, "{name}: badge");
            assert_eq!(pending.motion_count(), motion, "{name}: motion");
            assert_eq!(pending.jump_count(), jump, "{name}: jump");
        }
    }

    #[test]
    fn zero_count_cannot_be_constructed_through_the_module_api() {
        assert_eq!(Count::new(0), None);
        assert_eq!(apply(&[Operation::Digit(0)]), Pending::Idle);
        assert_eq!(
            apply(&[Operation::Arm(Prefix::G), Operation::Digit(0)]),
            Pending::Idle
        );
    }
}
