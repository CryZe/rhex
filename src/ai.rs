use rand;
use rand::Rng;
use std::sync::{Arc,mpsc};

use hex2dext::algo::bfs;

use hex2d::{self, Coordinate};
use hex2d as h2d;
use game;
use actor;


fn roam() -> game::Action {
    match rand::thread_rng().gen_range(0, 10) {
        0 => game::Action::Turn(h2d::Angle::Right),
        1 => game::Action::Turn(h2d::Angle::Left),
        2 => game::Action::Move(h2d::Angle::Forward),
        _ => game::Action::Wait,
    }
}


fn closest_reachable<F>(gstate : &game::State, start : Coordinate, max_distance : i32, cond : F) -> Option<(Coordinate, Coordinate)>
    where F : Fn(Coordinate) -> bool
{
    let mut bfs = bfs::Traverser::new(
        |pos| pos == start || (gstate.tile_map_or(pos, false, |t| t.is_passable())
                               && pos.distance(start) < max_distance && !gstate.occupied(pos)),
                               cond,
                               start
                               );
    bfs.find().map(|pos| (pos, bfs.backtrace_last(pos).unwrap()))
}


fn pony_follow(astate : &actor::State, gstate : &game::State) -> game::Action {

        let start = astate.pos;


        let player_pos = closest_reachable(gstate, start, 10,
            |pos| gstate.actor_map_or(pos, false, &|a| a.is_player())
            );

        let player_pos = if let Some((dst, _)) = player_pos {
            let distance = dst.distance(start);
            if distance < 3 {
                closest_reachable(gstate, start, 10, |pos| pos.distance(dst) == 3 && gstate.passable(pos))
            } else if distance < 5 {
                None
            } else {
                player_pos
            }
        } else {
            player_pos
        };

        if let Some((_, neigh)) = player_pos {
            let ndir = astate.pos.direction_to_cw(neigh).expect("bfs gave me trash");

            if ndir == astate.dir {
                return game::Action::Move(hex2d::Angle::Forward)
            } else {
                return game::Action::Turn(ndir - astate.dir)
            }
        } else {
            roam()
        }
}

pub fn run(
    req : mpsc::Receiver<(Arc<actor::State>, Arc<game::State>)>,
    rep : mpsc::Sender<(Arc<actor::State>, game::Action)>
    )
{

    loop {
        let (astate, gstate) = req.recv().unwrap();

        let action = match astate.behavior {
            actor::Behavior::Ai => roam(),
            actor::Behavior::Pony => pony_follow(&astate, &gstate),
            _ => panic!(),
        };

        rep.send((astate, action)).unwrap();
    }
}
