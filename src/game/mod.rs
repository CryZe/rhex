use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::Entry;
use std::sync::{Arc};

use hex2d::{Coordinate, Direction, Angle, Position};
use actor;
use generate;
use hex2dext::algo;

use self::tile::{Tile};

pub mod area;
pub mod tile;
pub mod controller;

pub use self::controller::Controller;

pub type Map = HashMap<Coordinate, tile::Tile>;
pub type Actors = HashMap<Coordinate, Arc<actor::State>>;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Action {
    Wait,
    Turn(Angle),
    Move(Angle),
    Spin(Angle),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Stage {
    ST1,
    ST2,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct State {
    pub actors: Arc<Actors>,
    pub actors_done: Arc<HashSet<Coordinate>>,
    pub actors_dead: Arc<Vec<Arc<actor::State>>>,
    pub map : Arc<Map>,
    pub light_map: Arc<HashMap<Coordinate, u32>>,
    pub turn : u64,
}

impl State {
    pub fn new() -> State {

        let cp = Coordinate::new(0, 0);
        let (map, actors) = generate::DungeonGenerator.generate_map(cp, 400);

        State {
            actors: Arc::new(actors),
            actors_done: Arc::new(HashSet::new()),
            actors_dead: Arc::new(Vec::new()),
            map: Arc::new(map),
            turn: 0,
            light_map: Arc::new(HashMap::new()),
        }
    }

    pub fn recalculate_light_map(&mut self) {
        let mut light_map : HashMap<Coordinate, u32> = HashMap::new();

        for (pos, tile) in &*self.map {
            let light = tile.light;
            if light > 0 {
                algo::los::los(
                    &|coord| {
                        if coord == *pos {
                            0
                        } else {
                            self.tile_at(coord).map_or(light, |tile| tile.opaqueness())
                        }
                    },
                    &mut |coord, light| {
                        match light_map.entry(coord) {
                            Entry::Occupied(mut entry) => {
                                let val = entry.get_mut();
                                if light as u32 > *val {
                                    *val = light as u32;
                                }
                            },
                            Entry::Vacant(entry) => {
                                entry.insert(light as u32);
                            },
                        }
                    },
                    light, *pos, Direction::all()
                    );
            }
        }

        for (pos, astate) in &*self.actors {
            if astate.light > 0 {
                algo::los::los(
                    &|coord| {
                        if coord == *pos {
                            0
                        } else {
                            self.tile_at(coord).map_or(astate.light as i32, |tile| tile.opaqueness())
                        }
                    },
                    &mut |coord, light| {
                        match light_map.entry(coord) {
                            Entry::Occupied(mut entry) => {
                                let val = entry.get_mut();
                                if light as u32 > *val {
                                    *val = light as u32;
                                }
                            },
                            Entry::Vacant(entry) => {
                                entry.insert(light as u32);
                            },
                        }
                    },
                    astate.light as i32, *pos, Direction::all()
                    );
            }
        }

        self.light_map = Arc::new(light_map);
    }

    pub fn spawn(&self, coord : Coordinate, behavior : actor::Behavior, light : u32) -> State {

        let mut actors = self.actors.clone().make_unique().clone();

        let pos = Position::new(coord, Direction::XY);

        actors.insert(pos.coord, Arc::new(
                actor::State::new(behavior, pos).add_light(light)
                ));

        State {
            actors: Arc::new(actors),
            actors_done: self.actors_done.clone(),
            actors_dead: self.actors_dead.clone(),
            map: self.map.clone(),
            turn: self.turn,
            light_map: self.light_map.clone(),
        }
    }

    pub fn spawn_player(&self) -> State {
        self.spawn(Coordinate::new(0, 0), actor::Behavior::Player, 0)
    }

    pub fn spawn_pony(&self, pos : Coordinate) -> State {
        self.spawn(pos, actor::Behavior::Pony, 7)
    }

    pub fn actor_act(&self, stage : Stage,
                     astate : &actor::State,
                     action : Action) -> Option<State> {
        let new_pos = astate.pos_after_action(action);

        if astate.pos == new_pos {
            // we did nothing
            None
        } else if astate.pos.coord != new_pos.coord && self.actors.contains_key(&new_pos.coord) {
            // that was an attack!
            if stage != Stage::ST1 {
                return None;
            }
            let mut actors = self.actors.clone().make_unique().clone();
            let mut actors_done = self.actors_done.clone().make_unique().clone();

            let target = &self.actors[new_pos.coord];
            let target_new_state = target.hit();
            actors.remove(&new_pos.coord);
            actors.insert(target_new_state.pos.coord, Arc::new(target_new_state));

            actors_done.insert(astate.pos.coord);

            let ret = State {
                actors: Arc::new(actors),
                actors_done: Arc::new(actors_done),
                actors_dead: self.actors_dead.clone(),
                map: self.map.clone(),
                turn: self.turn,
                light_map: self.light_map.clone(),
            };
            Some(ret)
        } else if self.tile_map_or(new_pos.coord, false, |t| t.type_ == tile::Door(false)) {

            if stage != Stage::ST2 {
                return None
            }

            // open the door
            let mut map = self.map.clone().make_unique().clone();

            map.insert(new_pos.coord, Tile::new(tile::Door(true)));

            Some(State {
                actors: self.actors.clone(),
                actors_done: self.actors_done.clone(),
                actors_dead: self.actors_dead.clone(),
                map: Arc::new(map),
                turn: self.turn,
                light_map: self.light_map.clone(),
            })
        } else if astate.pos.coord == new_pos.coord || self.is_passable(new_pos.coord) {
            // we've moved
            if stage != Stage::ST2 {
                return None
            }

            let mut actors = self.actors.clone().make_unique().clone();
            let actor_new_state = astate.change_position(new_pos);

            actors.remove(&astate.pos.coord);
            actors.insert(actor_new_state.pos.coord, Arc::new(actor_new_state));

            let ret = State {
                actors: Arc::new(actors),
                actors_done: self.actors_done.clone(),
                actors_dead: self.actors_dead.clone(),
                map: self.map.clone(),
                turn: self.turn,
                light_map: self.light_map.clone(),
            };
            Some(ret)
        } else {
            // we hit the wall or something
            None
        }
    }

    pub fn act(&self, stage : Stage,
               acoord : Coordinate, action : Action) -> State {

        if self.actors_done.contains(&acoord) {
            return self.clone()
        }

        let astate = &self.actors[acoord];

        if let Some(state) = self.actor_act(stage, astate, action) {
            state
        } else {
            self.clone()
        }
    }

    pub fn pre_tick(&self) -> State {
        let mut actors = HashMap::new();
        for (&coord, a) in self.actors.iter() {
                let mut a = a.clone().make_unique().clone();
                a.pre_tick(self);
                actors.insert(coord, Arc::new(a));
        }

        let mut ret = self.clone();

        ret.actors = Arc::new(actors);

        ret
    }

    /// Advance one turn (increase the turn counter) and do some maintenance
    pub fn post_tick(&self) -> State {
        // filter out the dead
        let mut actors = HashMap::new();
        let mut actors_dead = self.actors_dead.clone().make_unique().clone();

        for (&coord, a) in self.actors.iter() {
            if a.is_dead() {
                if a.behavior == actor::Behavior::Player {
                    actors_dead.push(a.clone());
                }
            } else {
                actors.insert(coord, a.clone());
            }
        }

        let mut ret = State {
            actors: Arc::new(actors),
            actors_done: Arc::new(HashSet::new()),
            actors_dead: Arc::new(actors_dead),
            map: self.map.clone(),
            turn: self.turn + 1,
            light_map: Arc::new(HashMap::new()),
        };
        ret.recalculate_light_map();

        let mut actors = HashMap::new();

        for (&coord, a) in ret.actors.iter() {
                let mut a = a.clone().make_unique().clone();
                a.post_tick(&ret);
                actors.insert(coord, Arc::new(a));
        }

        State {
            actors: Arc::new(actors),
            actors_done: Arc::new(HashSet::new()),
            actors_dead: ret.actors_dead.clone(),
            map: ret.map.clone(),
            turn: ret.turn,
            light_map: ret.light_map.clone(),
        }
    }

    pub fn actor_map_or<R, F : Fn(&actor::State) -> R>
        (&self, pos : Coordinate, def: R, cond : F) -> R
    {
            self.actors.get(&pos).map_or(def, |a| cond(a))
    }

    pub fn tile_at(&self, pos : Coordinate) -> Option<&tile::Tile> {
        self.map.get(&pos)
    }

    pub fn tile_map_or<R, F : Fn(&tile::Tile) -> R>(&self, pos : Coordinate, def: R, f : F) -> R {
        self.map.get(&pos).map_or(def, |a| f(a))
    }

    pub fn is_occupied(&self, pos : Coordinate) -> bool {
        self.actors.contains_key(&pos)
    }

    pub fn is_passable(&self, pos : Coordinate) -> bool {
        !self.is_occupied(pos) && self.tile_map_or(pos, false, |t| t.is_passable())
    }

    pub fn light(&self, pos : Coordinate) -> u32 {
        self.light_map.get(&pos).map_or(0, |l| *l)
    }
}

