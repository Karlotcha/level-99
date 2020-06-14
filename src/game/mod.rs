use anyhow::*;
use parking_lot::RwLock;
use serenity::model::id::{ChannelId, UserId};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

pub mod pool;
mod quiz;
pub mod team;

use self::quiz::definition::QuizDefinition;
use self::quiz::Quiz;
use self::team::{sanitize_name, Team, TeamId, TeamsHandle};
use crate::output::{OutputPipe, Recipient};

enum Phase {
    Startup,
    Setup,
    Quiz(Quiz),
}

pub struct Game {
    current_phase: Phase,
    teams: TeamsHandle,
    output_pipe: Arc<RwLock<OutputPipe>>,
    paused: bool,
}

impl Game {
    pub fn new(output_pipe: OutputPipe, teams: TeamsHandle) -> Game {
        let mut game = Game {
            current_phase: Phase::Startup,
            output_pipe: Arc::new(RwLock::new(output_pipe)),
            paused: false,
            teams,
        };
        game.set_current_phase(Phase::Setup);
        game
    }

    fn set_current_phase(&mut self, phase: Phase) {
        self.current_phase = phase;
    }

    pub fn tick(&mut self, dt: Duration) {
        if self.paused {
            return;
        }
        match &mut self.current_phase {
            Phase::Startup | Phase::Setup => (),
            Phase::Quiz(quiz) => {
                quiz.tick(dt);
                if quiz.is_over() {
                    self.set_current_phase(Phase::Setup);
                }
            }
        };
    }

    pub fn begin(&mut self, quiz_path: &Path) -> Result<()> {
        match &self.current_phase {
            Phase::Setup => {
                let definition = QuizDefinition::open(quiz_path)?;
                let quiz = Quiz::new(definition, self.teams.clone(), self.output_pipe.clone());
                self.set_current_phase(Phase::Quiz(quiz));
                Ok(())
            }
            _ => Err(anyhow!("Cannot call begin outside of setup phase")),
        }
    }

    pub fn skip(&mut self) -> Result<()> {
        match &mut self.current_phase {
            Phase::Quiz(q) => {
                q.skip_phase();
                Ok(())
            }
            _ => Err(anyhow!("There is no quiz in progress")),
        }
    }

    pub fn guess(&mut self, player: UserId, guess: &str) -> Result<()> {
        let team_id = self
            .get_player_team(player)
            .context("Player is not on a team")?;

        match &mut self.current_phase {
            Phase::Quiz(quiz) => {
                quiz.guess(&team_id, guess)?;
                Ok(())
            }
            _ => Err(anyhow!("Cannot submit answers during setup phase")),
        }
    }

    pub fn disband_team(&mut self, team_name: &str) -> Result<()> {
        let team_name = sanitize_name(team_name)?;
        let team_id = TeamId::TeamName(team_name);
        let mut teams = self.teams.write();
        let index = teams
            .iter()
            .position(|t| t.id == team_id)
            .context("Team not found")?;
        teams.swap_remove(index);
        Ok(())
    }

    pub fn join_team(&mut self, player: UserId, team_name: &str) -> Result<()> {
        let is_setup_phase = match &self.current_phase {
            Phase::Setup => true,
            _ => false,
        };
        let is_player_on_team = self.get_player_team(player).is_some();
        if is_player_on_team && !is_setup_phase {
            return Err(anyhow!("Team can not be changed during a quiz"));
        }

        let mut teams = self.teams.write();

        // Remove player from existing team
        for team in teams.iter_mut() {
            team.players.remove(&player);
        }

        // Put player on his desired team
        let team_name = sanitize_name(team_name)?;
        let team_id = TeamId::TeamName(team_name);
        let mut team = teams.iter_mut().find(|team| team.id == team_id);
        if team.is_none() {
            let new_team = Team::new(team_id);
            teams.push(new_team);
            team = Some(teams.iter_mut().last().expect("Team not found"));
        }
        if let Some(team) = team {
            team.players.insert(player);
        }

        // Remove empty teams
        teams.retain(|t| !t.players.is_empty());

        Ok(())
    }

    pub fn adjust_score(&mut self, team_id: TeamId, delta: i32) -> Result<()> {
        let mut teams = self.teams.write();
        let team = teams
            .iter_mut()
            .find(|t| t.id == team_id)
            .context("Team not found")?;
        team.update_score(delta);
        let output_pipe = self.output_pipe.read();
        output_pipe.say(
            &Recipient::AllTeams,
            &format!(
                "Team {}'s score was updated to {} points",
                team.get_display_name(),
                team.score
            ),
        );
        Ok(())
    }

    pub fn reset_teams(&mut self) {
        self.teams.write().clear();
        let output_pipe = self.output_pipe.read();
        output_pipe.say(&Recipient::AllTeams, "Teams were reset");
    }

    pub fn reset_scores(&mut self) {
        {
            let mut teams = self.teams.write();
            for team in teams.iter_mut() {
                team.score = 0;
            }
        }
        let output_pipe = self.output_pipe.read();
        output_pipe.say(&Recipient::AllTeams, "Scores were reset");
    }

    pub fn pause(&mut self) {
        if !self.paused {
            self.paused = true;
            let output_pipe = self.output_pipe.read();
            output_pipe.say(
                &Recipient::AllTeams,
                "The game is now paused, use `!unpause` to resume.",
            );
        }
    }

    pub fn unpause(&mut self) {
        if self.paused {
            self.paused = false;
            let output_pipe = self.output_pipe.read();
            output_pipe.say(&Recipient::AllTeams, "The game has resumed.");
        }
    }

    pub fn update_team_channels(&self, channel_ids: HashMap<TeamId, ChannelId>) {
        let mut output_pipe = self.output_pipe.write();
        output_pipe.update_team_channels(channel_ids);
    }

    fn get_player_team(&self, player: UserId) -> Option<TeamId> {
        let teams = self.teams.read();
        teams
            .iter()
            .find(|t| t.players.contains(&player))
            .and_then(|t| Some(t.id.clone()))
    }

    pub fn get_teams(&self) -> Vec<Team> {
        self.teams.read().clone()
    }
}
