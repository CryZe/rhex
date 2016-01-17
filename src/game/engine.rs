use super::{Location, Action};
use super::{actor};
use util;
use ai::{self, Ai};

pub struct Engine {
    location_cur : usize,
    locations : Vec<Location>,

    ids_to_move : Vec<actor::Id>,
}

impl Engine {
    pub fn new() -> Self {
        let location = Location::new();
        Engine {
            location_cur : 0,
            locations: vec!(location),
            ids_to_move: vec!(),
        }
    }

    pub fn current_location(&self) -> &Location {
        &self.locations[self.location_cur]
    }

    pub fn current_location_mut(&mut self) -> &mut Location {
        &mut self.locations[self.location_cur]
    }

    // TODO: Move field to engine
    pub fn turn(&self) -> u64 {
        self.current_location().turn
    }

    pub fn spawn(&mut self) {
        self.current_location_mut().spawn_player(util::random_pos(0, 0));
    }

    pub fn needs_player_input(&self) -> bool {
        self.ids_to_move.is_empty()
    }

    pub fn checks_after_act(&mut self) {
        if self.ids_to_move.is_empty() {
            self.current_location_mut().post_turn()
        }
    }

    // player first move
    pub fn player_act(&mut self, action : Action) {
        assert!(self.needs_player_input());

        let player_id = self.current_location().player_id();

        self.current_location_mut().act(player_id, action);

        self.ids_to_move = self.current_location().actors_alive_ids()
            .iter()
            .cloned()
            .filter(|&id| id != player_id).collect();

        self.checks_after_act()
    }

    // then everybody else one by one
    pub fn one_actor_act(&mut self) -> actor::Id {
        assert!(!self.needs_player_input());

        let actor_id = self.ids_to_move.pop().unwrap();

        let player_id = self.current_location().player_id();
        assert!(actor_id != player_id);

        let mut ai = ai::Simple;
        let action = ai.action(actor_id, self);
        self.current_location_mut().act(actor_id, action);

        self.checks_after_act();

        actor_id
    }
}
