//! What the user's words do to the image generator, decided locally. The
//! application acts (switch, keep a request) and the companion only talks
//! about it through a receipt in its system text; it never switches anything.
use sherka_core::illustration::{Config, ImageIntent, ImagePrompt, ImageReceipt, ImageUnavailable};

#[derive(Debug, Clone, Copy)]
pub struct Facts {
    pub enabled: bool,
    pub ready: bool,
    pub trio: bool,
    /// An image request is waiting for the generator.
    pub wished: bool,
}

/// What the companion is told this turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tell {
    /// Nothing about images (three-way conversation without a request).
    Silent,
    /// The generator state (nothing when no model is set up), with a receipt.
    State(Option<ImageReceipt>),
    /// Only why the application cannot do it.
    Only(ImageUnavailable),
}

/// What happens to the waiting image request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wish {
    Keep,
    Clear,
    /// The user's message becomes the waiting request.
    Store,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    /// Switch the generator before the reply (the same switch as the button).
    pub switch: Option<bool>,
    pub tell: Tell,
    pub wish: Wish,
}

impl Plan {
    /// The switch failed and nothing changed; a request stays available.
    pub fn failed(self) -> Self {
        Self {
            switch: None,
            tell: Tell::Only(ImageUnavailable::SwitchFailed),
            wish: if self.tell == Tell::State(Some(ImageReceipt::Preparing)) {
                Wish::Store
            } else {
                self.wish
            },
        }
    }

    /// The receipt this turn keeps for the post-reply decision.
    pub fn receipt(&self) -> Option<ImageReceipt> {
        match self.tell {
            Tell::Silent => None,
            Tell::State(receipt) => receipt,
            Tell::Only(reason) => Some(ImageReceipt::Unavailable(reason)),
        }
    }
}

/// The decision for a normal user turn. `asks_on` is whether a request also
/// asks to switch the generator on. In a trio the switch is never touched.
pub fn plan(intent: Option<ImageIntent>, asks_on: bool, facts: Facts) -> Plan {
    use ImageIntent::{Make, SwitchOff, SwitchOn};
    use ImageReceipt::{NeedsSwitchOn, Preparing, SwitchedOff, SwitchedOn, SwitchedOnWithRequest};
    use ImageUnavailable::{NotConfigured, ThreeWay};
    let plan = |switch, tell, wish| Plan { switch, tell, wish };
    if facts.trio {
        return match intent {
            None => plan(None, Tell::Silent, Wish::Clear),
            Some(SwitchOff) => plan(None, Tell::Only(ThreeWay), Wish::Clear),
            Some(_) => plan(None, Tell::Only(ThreeWay), Wish::Keep),
        };
    }
    let state = |receipt| Tell::State(Some(receipt));
    match intent {
        None => plan(None, Tell::State(None), Wish::Clear),
        Some(SwitchOn) if !facts.ready => plan(None, Tell::Only(NotConfigured), Wish::Keep),
        Some(SwitchOn) => plan(
            (!facts.enabled).then_some(true),
            state(if facts.wished {
                SwitchedOnWithRequest
            } else {
                SwitchedOn
            }),
            Wish::Keep,
        ),
        Some(SwitchOff) if !facts.enabled && !facts.ready => {
            plan(None, Tell::Only(NotConfigured), Wish::Clear)
        }
        Some(SwitchOff) => plan(
            facts.enabled.then_some(false),
            state(SwitchedOff),
            Wish::Clear,
        ),
        Some(Make) if !facts.ready => plan(None, Tell::Only(NotConfigured), Wish::Clear),
        Some(Make) if facts.enabled => plan(None, state(Preparing), Wish::Clear),
        Some(Make) if asks_on => plan(Some(true), state(Preparing), Wish::Clear),
        Some(Make) => plan(None, state(NeedsSwitchOn), Wish::Store),
    }
}

/// The image part of the system text for `tell`. `enabled` is the live switch
/// once the plan was applied.
pub fn prompt(tell: Tell, config: &Config, enabled: bool) -> Option<ImagePrompt> {
    match tell {
        Tell::Silent => None,
        Tell::State(receipt) => ImagePrompt::new(config, enabled, receipt),
        Tell::Only(reason) => Some(ImagePrompt::unavailable(reason)),
    }
}

/// Whether the usual illustration of the exchange follows the reply. A command
/// exchange is not illustrated; a request waiting for the generator is, as soon
/// as the generator is on.
pub fn illustrate_after_reply(receipt: Option<ImageReceipt>) -> bool {
    !matches!(
        receipt,
        Some(
            ImageReceipt::SwitchedOn
                | ImageReceipt::SwitchedOff
                | ImageReceipt::NeedsSwitchOn
                | ImageReceipt::Unavailable(_)
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ImageIntent::{Make, SwitchOff, SwitchOn};
    use ImageReceipt::{NeedsSwitchOn, Preparing, SwitchedOff, SwitchedOn, SwitchedOnWithRequest};
    use ImageUnavailable::{NotConfigured, SwitchFailed, ThreeWay};

    fn facts(enabled: bool, ready: bool, trio: bool, wished: bool) -> Facts {
        Facts {
            enabled,
            ready,
            trio,
            wished,
        }
    }

    fn p(switch: Option<bool>, tell: Tell, wish: Wish) -> Plan {
        Plan { switch, tell, wish }
    }
    fn state(receipt: ImageReceipt) -> Tell {
        Tell::State(Some(receipt))
    }

    #[test]
    fn switching_on_is_done_by_the_application_with_the_right_receipt() {
        let off = facts(false, true, false, false);
        assert_eq!(
            plan(Some(SwitchOn), false, off),
            p(Some(true), state(SwitchedOn), Wish::Keep)
        );
        let waiting = facts(false, true, false, true);
        assert_eq!(
            plan(Some(SwitchOn), false, waiting),
            p(Some(true), state(SwitchedOnWithRequest), Wish::Keep)
        );
        // Already on: nothing to switch, the command exchange is not drawn.
        let on = facts(true, true, false, false);
        assert_eq!(
            plan(Some(SwitchOn), false, on),
            p(None, state(SwitchedOn), Wish::Keep)
        );
        let unset = facts(false, false, false, false);
        assert_eq!(
            plan(Some(SwitchOn), false, unset),
            p(None, Tell::Only(NotConfigured), Wish::Keep)
        );
    }

    #[test]
    fn switching_off_cancels_and_forgets_the_waiting_request() {
        assert_eq!(
            plan(Some(SwitchOff), false, facts(true, true, false, false)),
            p(Some(false), state(SwitchedOff), Wish::Clear)
        );
        assert_eq!(
            plan(Some(SwitchOff), false, facts(false, true, false, true)),
            p(None, state(SwitchedOff), Wish::Clear)
        );
        assert_eq!(
            plan(Some(SwitchOff), false, facts(false, false, false, false)),
            p(None, Tell::Only(NotConfigured), Wish::Clear)
        );
        // A generator that is on but lost its model can still be switched off.
        assert_eq!(
            plan(Some(SwitchOff), false, facts(true, false, false, false)).switch,
            Some(false)
        );
    }

    #[test]
    fn a_request_follows_the_switch() {
        // On: the normal illustration follows the reply.
        assert_eq!(
            plan(Some(Make), false, facts(true, true, false, false)),
            p(None, state(Preparing), Wish::Clear)
        );
        // Off and not asked to switch on: kept until the user switches on.
        assert_eq!(
            plan(Some(Make), false, facts(false, true, false, false)),
            p(None, state(NeedsSwitchOn), Wish::Store)
        );
        // Off but the same message asks for it: switched on first.
        assert_eq!(
            plan(Some(Make), true, facts(false, true, false, false)),
            p(Some(true), state(Preparing), Wish::Clear)
        );
        // Not set up.
        assert_eq!(
            plan(Some(Make), true, facts(false, false, false, false)),
            p(None, Tell::Only(NotConfigured), Wish::Clear)
        );
        assert_eq!(
            plan(Some(Make), false, facts(true, false, false, false)).tell,
            Tell::Only(NotConfigured)
        );
    }

    #[test]
    fn without_an_intent_the_state_is_told_and_a_waiting_request_is_dropped() {
        assert_eq!(
            plan(None, false, facts(true, true, false, true)),
            p(None, Tell::State(None), Wish::Clear)
        );
        assert_eq!(
            plan(None, false, facts(false, true, false, false)).switch,
            None
        );
    }

    #[test]
    fn a_three_way_conversation_never_touches_the_switch() {
        for intent in [Some(SwitchOn), Some(SwitchOff), Some(Make)] {
            for enabled in [false, true] {
                for asks_on in [false, true] {
                    let decision = plan(intent, asks_on, facts(enabled, true, true, false));
                    assert_eq!(decision.switch, None);
                    assert_eq!(decision.tell, Tell::Only(ThreeWay));
                    assert_ne!(decision.wish, Wish::Store);
                }
            }
        }
        assert_eq!(
            plan(None, false, facts(true, true, true, true)),
            p(None, Tell::Silent, Wish::Clear)
        );
    }

    #[test]
    fn a_failed_switch_changes_nothing_and_keeps_the_request() {
        let switching = plan(Some(SwitchOn), false, facts(false, true, false, true));
        let failed = switching.failed();
        assert_eq!(
            failed,
            p(None, Tell::Only(SwitchFailed), Wish::Keep),
            "the waiting request survives"
        );
        assert_eq!(
            failed.receipt(),
            Some(ImageReceipt::Unavailable(SwitchFailed))
        );
        // A request that wanted the switch is kept for the user's retry.
        let make = plan(Some(Make), true, facts(false, true, false, false)).failed();
        assert_eq!(make, p(None, Tell::Only(SwitchFailed), Wish::Store));
    }

    #[test]
    fn the_receipt_is_what_the_companion_is_told() {
        let decision = plan(Some(SwitchOn), false, facts(false, true, false, false));
        assert_eq!(decision.receipt(), Some(SwitchedOn));
        assert_eq!(
            plan(None, false, facts(true, true, false, false)).receipt(),
            None
        );
        assert_eq!(
            plan(None, false, facts(true, true, true, false)).receipt(),
            None
        );
        assert_eq!(
            plan(Some(Make), false, facts(true, true, true, false)).receipt(),
            Some(ImageReceipt::Unavailable(ThreeWay))
        );
    }

    #[test]
    fn the_prompt_follows_the_decision() {
        let config = Config {
            checkpoint: Some("models/Illustrious-XL.safetensors".into()),
            ..Default::default()
        };
        let on = prompt(state(SwitchedOn), &config, true).unwrap();
        assert!(on.enabled && on.receipt == Some(SwitchedOn));
        let state_only = prompt(Tell::State(None), &config, false).unwrap();
        assert!(!state_only.enabled && state_only.receipt.is_none());
        assert!(prompt(Tell::Silent, &config, true).is_none());
        // Not set up: no state, only the reason.
        assert!(prompt(Tell::State(None), &Config::default(), false).is_none());
        let reason = prompt(Tell::Only(NotConfigured), &Config::default(), false).unwrap();
        assert_eq!(
            reason.receipt,
            Some(ImageReceipt::Unavailable(NotConfigured))
        );
    }

    #[test]
    fn only_ordinary_and_requested_exchanges_are_illustrated_after_the_reply() {
        assert!(illustrate_after_reply(None));
        assert!(illustrate_after_reply(Some(Preparing)));
        assert!(illustrate_after_reply(Some(SwitchedOnWithRequest)));
        assert!(!illustrate_after_reply(Some(SwitchedOn)));
        assert!(!illustrate_after_reply(Some(SwitchedOff)));
        assert!(!illustrate_after_reply(Some(NeedsSwitchOn)));
        assert!(!illustrate_after_reply(Some(ImageReceipt::Unavailable(
            ThreeWay
        ))));
    }
}
