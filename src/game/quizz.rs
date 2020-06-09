use anyhow::*;
use std::path::Path;
use std::time::Duration;

use crate::game::definition::*;
use crate::game::output::{OutputPipe, Payload};
use crate::game::settings::*;

trait Step {
    fn begin();
    fn tick(dt: Duration);
}

#[derive(Debug)]
enum QuizzStep {
    Cooldown(CooldownStep),
    Vote,
    Question,
}

#[derive(Debug)]
struct CooldownStep {
    time_elapsed: Duration,
    time_to_wait: Duration,
}

impl CooldownStep {
    pub fn new(duration: Duration) -> Self {
        CooldownStep {
            time_elapsed: Duration::default(),
            time_to_wait: duration,
        }
    }

    pub fn tick(&mut self, dt: Duration) {
        self.time_elapsed += dt;
    }

    pub fn is_over(&self) -> bool {
        self.time_elapsed >= self.time_to_wait
    }
}

pub struct Quizz {
    settings: Settings,
    remaining_questions: Vec<Question>,
    current_step: QuizzStep,
}

impl Quizz {
    fn new(definition: QuizzDefinition) -> Quizz {
        let settings: Settings = Default::default();
        Quizz {
            remaining_questions: definition.get_questions().clone(),
            current_step: QuizzStep::Cooldown(CooldownStep::new(settings.cooldown_duration)),
            settings,
        }
    }

    pub fn load(source: &Path) -> Result<Quizz> {
        let definition = QuizzDefinition::open(source)?;
        Ok(Quizz::new(definition))
    }

    fn set_current_step(&mut self, step: QuizzStep) {
        println!("Current step: {:?}", step);
        self.current_step = step;
    }

    pub fn tick(&mut self, dt: Duration, output_pipe: &mut OutputPipe) {
        match &mut self.current_step {
            QuizzStep::Cooldown(cooldown_state) => {
                cooldown_state.tick(dt);
                if cooldown_state.is_over() {
                    // TODO give a Enter and End method to each step to guarantee events are emitted when they need to
                    self.set_current_step(QuizzStep::Question);
                    output_pipe.push(Payload::Text("Time for a question!".into()));
                }
            }
            _ => (),
        };
    }
}
