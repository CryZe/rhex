use std::collections::{HashMap, HashSet};
use std::collections::hash_state::{DefaultState};
use std::sync::Arc;

use generate;
use fnv::FnvHasher;

use hex2dext::algo;
use simplemap::SimpleMap;
use hex2d::{Coordinate, Position, Direction};

use super::tile;
use super::item::Item;
use super::Action;
use super::actor::{self, Actor};
use super::{LightMap, Map, Items};
use super::{Noise};

#[derive(Clone, Debug)]
pub struct Location {
    pub actors_byid: HashMap<actor::Id, Actor, DefaultState<FnvHasher>>, // id -> State
    pub actors_coord_to_id: HashMap<Coordinate, u32, DefaultState<FnvHasher>>, // coord -> id
    pub actors_dead : HashSet<actor::Id, DefaultState<FnvHasher>>,
    pub actors_counter : u32,
    pub map : Arc<Map>,
    pub items: Items, // items on the floor
    pub light_map: LightMap, // light intensity at a given coordinate
    pub level : i32,
    player_id : Option<actor::Id>,
}

impl Location {
    pub fn new() -> Location {

        let cp = Coordinate::new(0, 0);
        let (map, gen_actors, items) = generate::DungeonGenerator::new(0).generate_map(cp, 400);

        let mut actors : HashMap<u32, Actor, DefaultState<FnvHasher>> = Default::default();
        let mut actors_pos : HashMap<Coordinate, u32, _> = Default::default();

        let mut actors_counter = 0u32;

        for (coord, astate) in gen_actors {
            actors_pos.insert(coord, actors_counter);
            actors.insert(actors_counter, astate);
            actors_counter += 1;
        }

        let loc = Location {
            actors_byid: actors,
            actors_coord_to_id: actors_pos,
            actors_counter: actors_counter,
            actors_dead: Default::default(),
            items: items,
            map: Arc::new(map),
            level: 0,
            light_map: LightMap::new(),
            player_id: None,
        };

       loc
    }

    pub fn player_id(&self) -> actor::Id {
        self.player_id.unwrap()
    }

    pub fn player(&self) -> &Actor {
        &self.actors_byid[&self.player_id.unwrap()]
    }

    /*
    pub fn next_level(&self) -> Location {
        let cp = Coordinate::new(0, 0);
        let (map, gen_actors, items) = generate::DungeonGenerator::new(self.level + 1).generate_map(cp, 400);

        let mut actors : HashMap<u32, Actor, DefaultState<FnvHasher>> = Default::default();
        let mut actors_pos : HashMap<Coordinate, u32, _> = Default::default();

        let mut actors_counter = 0;

        for (coord, astate) in gen_actors {
            actors_pos.insert(coord, actors_counter);
            actors.insert(actors_counter, astate);
            actors_counter += 1;
        }

        let mut player = None;
        let mut pony : Option<Actor> = None;

        for (_, astate) in self.actors_byid.iter() {
            if astate.is_player() {
                player = Some(astate.clone());
                break;
            }
        }

        for (_, astate) in self.actors_byid.iter() {
            /*
            if astate.race == Race::Pony {
                pony = Some(astate.clone());
                break;
            }
            */
        }

        let mut state = Location {
            actors_byid: actors,
            actors_coord_to_id: actors_pos,
            actors_counter: actors_counter,
            actors_dead: Default::default(),
            items: items,
            map: Arc::new(map),
            descend: false,
            level: self.level + 1,
            light_map: Default::default(),
        };

        {
            let mut player = player.unwrap();
            let pos = util::random_pos(0, 0);
            player.moved(self, pos);
            player.changed_level();
            state.spawn(player);
        }

        if let Some(mut pony) = pony {
            let pos = util::random_pos(-1, 0);
            pony.moved(self, pos);
            pony.changed_level();
            state.spawn(pony);
        }

        state
    }

        */
    pub fn recalculate_noise(&mut self) {
        for id in &self.actors_alive_ids() {
            let source_emission = self.actors_byid[id].noise_emision;
            if source_emission > 0 {
                let source_race = self.actors_byid[id].race;
                let source_coord = self.actors_byid[id].pos.coord;
                source_coord.for_each_in_range(source_emission, |coord| {
                    if let Some(&target_id) = self.actors_coord_to_id.get(&coord) {
                        self.actors_byid.get_mut(&target_id).unwrap()
                            .noise_hears(source_coord, Noise::Creature(source_race));
                    }
                });
            }
        }
    }

    pub fn actors_ids(&self) -> Vec<u32> {
        self.actors_byid.keys().cloned().collect()
    }

    pub fn actors_alive_ids(&self) -> Vec<u32> {
        self.actors_byid.keys().filter(|&id| !self.actors_byid[id].is_dead()).cloned().collect()
    }

    pub fn recalculate_light_map(&mut self) {
        let mut light_map : SimpleMap<Coordinate, u32> = Default::default();

        for (pos, tile) in self.map.iter() {
            let light = tile.light;
            if light > 0 {
                algo::los::los(
                    &|coord| {
                        if coord == *pos {
                            0
                        } else {
                            self.at(coord).tile().opaqueness()
                        }
                    },
                    &mut |coord, light| {
                        if light_map[coord] < light as u32 {
                            light_map[coord] = light as u32;
                        }
                    },
                    light, *pos, Direction::all()
                    );
            }
        }

        for (_, id) in &self.actors_coord_to_id {
            let astate = &self.actors_byid[id];
            let pos = astate.pos.coord;
            if astate.light_emision > 0 {
                algo::los::los(
                    &|coord| {
                        if coord == pos {
                            0
                        } else {
                            self.at(coord).tile().opaqueness()
                        }
                    },
                    &mut |coord, light| {
                       if light_map[coord] < light as u32 {
                           light_map[coord] = light as u32;
                       }
                    },
                    astate.light_emision as i32, pos, Direction::all()
                );
            }
        }

        self.light_map = light_map;
    }

    pub fn spawn(&mut self, mut astate : Actor) -> actor::Id {
        if self.actors_coord_to_id.contains_key(&astate.pos.coord) {
            // TODO: Find an alternative place
            unimplemented!();
        }
        self.pre_any_tick();
        let id = self.actors_counter;
        self.actors_counter += 1;

        debug_assert!(!self.actors_coord_to_id.contains_key(&astate.pos.coord));
        self.actors_coord_to_id.insert(astate.pos.coord, id);
        astate.pre_own_tick();
        let pos = astate.pos;
        astate.post_spawn(self);
        astate.post_own_tick(self);
        self.actors_byid.insert(id, astate);
        self.post_any_tick();

        id
    }

    pub fn remove(&mut self, id : actor::Id) -> Option<Actor> {
        let actor = self.actors_byid.remove(&id);

        let actor = if let Some(actor) = actor {
            actor
        } else {
            return None
        };

        self.actors_coord_to_id.remove(&actor.pos.coord);

        Some(actor)
    }

    pub fn spawn_player(&mut self, actor : Actor) -> actor::Id {
        assert!(actor.is_player());
        self.player_id = Some(self.spawn(actor));
        self.player_id.unwrap()
    }

    pub fn skip_act(&mut self, id : u32) {
        self.pre_any_tick();
        let mut actor = self.actors_byid.remove(&id).unwrap();
        actor.pre_own_tick();
        actor.post_own_tick(self);
        self.actors_byid.insert(id, actor);
        self.post_any_tick();
    }

    pub fn act(&mut self, id : u32, action : Action) {
        self.pre_any_tick();
        let mut actor = self.actors_byid.remove(&id).unwrap();

        if !actor.can_perform_action() {
            self.actors_byid.insert(id, actor);
            return;
        }

        actor.pre_own_tick();

        let new_pos = actor.pos_after_action(action);

        for &new_pos in &new_pos {
            let old_pos = actor.pos;

            if old_pos == new_pos {
                // no movement
                match action {
                    Action::Pick => {
                        let head = actor.head();
                        let item = self.at_mut(head).pick_item();

                        match item {
                            Some(item) => {
                                actor.add_item(item);
                            },
                            None => {},
                        }
                    },
                    Action::Equip(ch) => {
                        actor.equip_switch(ch);
                    },
                    Action::Descend => {
                        if self.at(actor.coord()).tile().feature == Some(tile::Feature::Stairs) {
                            actor.descend();
                        }
                    },
                    _ => {}
                }
            } else if action.could_be_attack() &&
                old_pos.coord != new_pos.coord &&
                    self.actors_coord_to_id.contains_key(&new_pos.coord)
                {
                    // we've tried to move into actor; attack?
                    if !actor.can_attack() {
                        break;
                    }
                    let dir = match action {
                        Action::Move(dir) => old_pos.dir + dir,
                        _ => old_pos.dir,
                    };

                    let target_id = self.actors_coord_to_id[&new_pos.coord];

                    let mut target = self.actors_byid.remove(&target_id).unwrap();
                    actor.attacks(dir, &mut target);
                    self.actors_byid.insert(target_id, target);
                    // Can't attack twice
                    break;
                } else if self.at(new_pos.coord).tile().feature == Some(tile::Door(false)
                    ) {
                    // walked into door: open it
                    let mut map = self.map.clone();
                    let mut map = Arc::make_mut(&mut map);
                    map[new_pos.coord].add_feature(tile::Door(true));
                    self.map = Arc::new(map.clone());
                    // Can't charge through the doors
                    break;
                } else if old_pos.coord == new_pos.coord &&
                    old_pos.dir != new_pos.dir
                {
                    // we've rotated
                    actor.moved(self, new_pos);
                } else if old_pos.coord != new_pos.coord &&
                    self.at(new_pos.coord).is_passable() &&
                    !self.actors_coord_to_id.contains_key(&new_pos.coord)
                {
                    // we've moved
                    actor.moved(self, new_pos);
                    // we will remove the previous position on post_tick, so that
                    // for the rest of this turn this actor can be found through both new
                    // and old coor
                    debug_assert!(!self.actors_coord_to_id.contains_key(&new_pos.coord));
                    self.actors_coord_to_id.insert(new_pos.coord, id);
                } else {
                    // we hit the wall or something
                }
        }
        actor.post_own_tick(self);
        self.actors_byid.insert(id, actor);
        self.actors_byid.get_mut(&id).unwrap().post_action(action);
        self.post_any_tick();
    }

    pub fn pre_any_tick(&mut self) {
        for id in self.actors_alive_ids() {
            let mut actor = self.actors_byid.remove(&id).unwrap();
            actor.pre_any_tick();
            self.actors_byid.insert(id, actor);
        }
    }

    pub fn post_any_tick(&mut self) {
        for id in self.actors_alive_ids() {
            let mut actor = self.actors_byid.remove(&id).unwrap();
            actor.post_any_tick(self);
            self.actors_byid.insert(id, actor);
        }

        for id in &self.actors_ids() {
            if self.actors_byid[id].is_dead() && !self.actors_dead.contains(&id){
                let mut a = self.actors_byid.remove(&id).unwrap();

                for (_, item) in a.items_backpack.iter() {
                    self.at_mut(a.pos.coord).drop_item(item.clone());
                }
                a.items_backpack.clear();

                for (_, &(_, ref item)) in a.items_equipped.iter() {
                    self.at_mut(a.pos.coord).drop_item(item.clone());
                }
                a.items_equipped.clear();

                self.actors_byid.insert(*id, a);

                self.actors_dead.insert(*id);
            }
        }

        self.actors_coord_to_id = self.actors_coord_to_id.iter()
            .filter(|&(_, id)| {
                !self.actors_byid[id].is_dead()
            }).map(|(_, id)| {
                (
                    self.actors_byid[id].pos.coord,
                    *id,
                    )
            }).collect();

        self.actors_coord_to_id.iter().map(|(coord, ref id)| {
            debug_assert!(self.actors_byid[*id].coord() == *coord);
        }).count();

        self.recalculate_light_map();
        self.recalculate_noise();

        /*
        for id in self.actors_alive_ids() {
            let mut actor = self.actors_byid.remove(&id).unwrap();
            self.actors_byid.insert(id, actor);
        }*/
    }

    pub fn post_turn(&mut self) {
    }

    pub fn at(&self, coord: Coordinate) -> At {
        At {
            coord: coord,
            state: self
        }
    }

    pub fn at_mut(&mut self, coord: Coordinate) -> AtMut {
        AtMut {
            coord: coord,
            state: self
        }
    }
}

pub struct At<'a> {
    coord : Coordinate,
    state : &'a Location,
}

impl<'a> At<'a> {
    // TODO: remove option
    pub fn tile(&self) -> &'a tile::Tile {
        &self.state.map[self.coord]
    }

    pub fn actor_map_or<R, F : Fn(&Actor) -> R>
        (&self, def: R, cond : F) -> R
    {
        self.state.actors_coord_to_id.get(&self.coord)
            .map(|&id| &self.state.actors_byid[&id])
            .map_or(def, |a| cond(&a))
    }

    pub fn item_map_or<R, F : Fn(&Box<Item>) -> R>
        (&self, def: R, cond : F) -> R
    {
        self.state.items.get(&self.coord).map_or(def, |i| cond(i))
    }

    pub fn is_occupied(&self) -> bool {
        self.state.actors_coord_to_id.contains_key(&self.coord)
    }

    pub fn is_passable(&self) -> bool {
        !self.is_occupied() && self.tile().is_passable()
    }

    pub fn _light(&self) -> u32 {
        self.state.light_map[self.coord]
    }

    pub fn light_as_seen_by(&self, astate : &Actor) -> u32 {
        let pl_coord = astate.pos.coord;

        let ownlight = self.state.light_map[self.coord];
        if self.state.map[self.coord].opaqueness() < 20 {
            ownlight
        } else {
            pl_coord.directions_to(self.coord)
                .iter()
                .map(|&dir| self.coord - dir)
                .map(|d_coord|
                     if self.state.map[d_coord].opaqueness() < 20 {
                         self.state.light_map[d_coord]
                     } else {
                         0
                     })
            .max().unwrap_or(0)
        }
    }

    pub fn item(&self) -> Option<&'a Item> {
        self.state.items.get(&self.coord).map(|i| &**i)
    }
}

pub struct AtMut<'a> {
    coord : Coordinate,
    state : &'a mut Location,
}

impl<'a> AtMut<'a> {
    pub fn drop_item(&mut self, item : Box<Item>) {
        let coord = {
            let mut bfs = algo::bfs::Traverser::new(
                |coord| self.state.at(coord).tile().is_passable(),
                |coord| self.state.at(coord).tile().is_passable() && self.state.items.get(&coord).is_none(),
                self.coord
                );

            bfs.find()
        };

        match coord {
            None => { /* destroy the item :/ */ },
            Some(coord) => {
                self.state.items.insert(coord, item);
            }
        }
    }

    pub fn pick_item(&mut self) -> Option<Box<Item>> {
        if self.state.items.get(&self.coord).is_some() {
            self.state.items.remove(&self.coord)
        } else {
            None
        }
    }
}
