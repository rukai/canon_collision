use treeflection::KeyedContextVec;

use canon_collision_lib::fighter::{Fighter, HurtBox, HitBox, CollisionBox, CollisionBoxRole, PowerShield};
use canon_collision_lib::stage::Surface;
use crate::player::Player;
use crate::entity::{Entities, EntityKey, EntityType};
use slotmap::SecondaryMap;

// def - player who was attacked
// atk - player who attacked

/// returns a list of hit results for each player
pub fn collision_check(entities: &Entities, fighters: &KeyedContextVec<Fighter>, surfaces: &[Surface]) -> SecondaryMap<EntityKey, Vec<CollisionResult>> {
    let mut result = SecondaryMap::<EntityKey, Vec<CollisionResult>>::new();
    for key in entities.keys() {
        result.insert(key, vec!());
    }

    'player_atk: for (player_atk_i, player_atk) in entities.iter() {
        let player_atk_xy = player_atk.public_bps_xy(entities, fighters, surfaces);
        let fighter_atk = &fighters[player_atk.entity_def_key()];
        for (player_def_i, player_def) in entities.iter() {
            let player_def_xy = player_def.public_bps_xy(entities, fighters, surfaces);
            if player_atk_i != player_def_i && player_atk.can_hit(player_def) && player_atk.hitlist().iter().all(|x| *x != player_def_i) {
                let fighter_def = &fighters[player_def.entity_def_key()];

                let frame_atk = &player_atk.relative_frame(fighter_atk, surfaces);
                let frame_def = &player_def.relative_frame(fighter_def, surfaces);
                let colboxes_atk = frame_atk.get_hitboxes();

                'hitbox_atk: for colbox_atk in &colboxes_atk {
                    if let CollisionBoxRole::Hit (ref hitbox_atk) = colbox_atk.role {
                        if let EntityType::Player(player_def) = &player_def.ty {
                            if colbox_shield_collision_check(player_atk_xy, colbox_atk, player_def_xy, player_def, fighter_def) {
                                result[player_atk_i].push(CollisionResult::HitShieldAtk {
                                    hitbox: hitbox_atk.clone(),
                                    power_shield: fighter_def.power_shield.clone(),
                                    player_def_i
                                });
                                result[player_def_i].push(CollisionResult::HitShieldDef {
                                    hitbox: hitbox_atk.clone(),
                                    power_shield: fighter_def.power_shield.clone(),
                                    player_atk_i
                                });
                                break 'hitbox_atk;
                            }
                        }

                        if hitbox_atk.enable_clang {
                            for colbox_def in frame_def.colboxes.iter() {
                                match &colbox_def.role {
                                // TODO: How do we only run the clang handler once?
                                    &CollisionBoxRole::Hit (ref hitbox_def) => {
                                        if let ColBoxCollisionResult::Hit (point) = colbox_collision_check(player_atk_xy, colbox_atk, player_def_xy, colbox_def) {
                                            let damage_diff = hitbox_atk.damage as i64 - hitbox_def.damage as i64; // TODO: retrieve proper damage with move staling etc

                                            if damage_diff >= 9 {
                                                result[player_atk_i].push(CollisionResult::Clang { rebound: hitbox_atk.enable_rebound });
                                                result[player_def_i].push(CollisionResult::HitAtk { hitbox: hitbox_atk.clone(), player_def_i: player_def_i, point });
                                            }
                                            else if damage_diff <= -9 {
                                                result[player_atk_i].push(CollisionResult::HitAtk { hitbox: hitbox_atk.clone(), player_def_i: player_def_i, point });
                                                result[player_def_i].push(CollisionResult::Clang { rebound: hitbox_def.enable_rebound });
                                            }
                                            else {
                                                result[player_atk_i].push(CollisionResult::Clang { rebound: hitbox_atk.enable_rebound });
                                                result[player_def_i].push(CollisionResult::Clang { rebound: hitbox_def.enable_rebound });
                                            }
                                            break 'player_atk;
                                        }
                                    }
                                    _ => { }
                                }
                            }
                        }

                        for colbox_def in frame_def.colboxes.iter() {
                            match colbox_collision_check(player_atk_xy, colbox_atk, player_def_xy, colbox_def) {
                                ColBoxCollisionResult::Hit (point) => {
                                    match &colbox_def.role {
                                        &CollisionBoxRole::Hurt (ref hurtbox) => {
                                            result[player_atk_i].push(CollisionResult::HitAtk { hitbox: hitbox_atk.clone(), player_def_i: player_def_i, point });
                                            result[player_def_i].push(CollisionResult::HitDef { hitbox: hitbox_atk.clone(), hurtbox: hurtbox.clone(), player_atk_i: player_atk_i });
                                            break 'player_atk;
                                        }
                                        &CollisionBoxRole::Invincible => {
                                            result[player_atk_i].push(CollisionResult::HitAtk { hitbox: hitbox_atk.clone(), player_def_i: player_def_i, point });
                                            break 'player_atk;
                                        }
                                        _ => { }
                                    }
                                }
                                ColBoxCollisionResult::Phantom (_) => {
                                    match &colbox_def.role {
                                        &CollisionBoxRole::Hurt (ref hurtbox) => {
                                            result[player_atk_i].push(CollisionResult::PhantomAtk (hitbox_atk.clone(), player_def_i));
                                            result[player_def_i].push(CollisionResult::PhantomDef (hitbox_atk.clone(), hurtbox.clone()));
                                            break 'player_atk;
                                        }
                                        _ => { }
                                    }
                                }
                                ColBoxCollisionResult::None => { }
                            }
                        }
                    }
                }

                for colbox_atk in &colboxes_atk {
                    match &colbox_atk.role {
                        &CollisionBoxRole::Grab => {
                            for colbox_def in &frame_def.colboxes[..] {
                                if let ColBoxCollisionResult::Hit (_) = colbox_collision_check(player_atk_xy, colbox_atk, player_def_xy, colbox_def) {
                                    result[player_atk_i].push(CollisionResult::GrabAtk (player_def_i));
                                    result[player_def_i].push(CollisionResult::GrabDef (player_atk_i));
                                    break 'player_atk;
                                }
                            }
                        }
                        _ => { }
                    }
                }

                // check colbox links
                // TODO
            }
        }
    }
    result
}

fn colbox_collision_check(player1_xy: (f32, f32), colbox1: &CollisionBox,  player2_xy: (f32, f32), colbox2: &CollisionBox) -> ColBoxCollisionResult {
    let x1 = player1_xy.0 + colbox1.point.0;
    let y1 = player1_xy.1 + colbox1.point.1;
    let r1 = colbox1.radius;

    let x2 = player2_xy.0 + colbox2.point.0;
    let y2 = player2_xy.1 + colbox2.point.1;
    let r2 = colbox2.radius;

    let check_distance = r1 + r2;
    let real_distance = ((x1 - x2).powi(2) + (y1 - y2).powi(2)).sqrt();

    if check_distance > real_distance {
        ColBoxCollisionResult::Hit (((x1 + x2) / 2.0, (y1 + y2) / 2.0))
    }
    else if check_distance + 0.01 > real_distance { // TODO: customizable phantom value
        ColBoxCollisionResult::Phantom (((x1 + x2) / 2.0, (y1 + y2) / 2.0))
    }
    else {
        ColBoxCollisionResult::None
    }
}

enum ColBoxCollisionResult {
    Hit ((f32, f32)),
    Phantom ((f32, f32)),
    None
}

fn colbox_shield_collision_check(player1_xy: (f32, f32), colbox1: &CollisionBox,  player2_xy: (f32, f32), player2: &Player, fighter2: &Fighter) -> bool {
    if let &Some(ref shield) = &fighter2.shield {
        if player2.is_shielding() {
            let x1 = player1_xy.0 + colbox1.point.0;
            let y1 = player1_xy.1 + colbox1.point.1;
            let r1 = colbox1.radius;

            let x2 = player2_xy.0 + player2.shield_offset_x + shield.offset_x;
            let y2 = player2_xy.1 + player2.shield_offset_y + shield.offset_y;
            let r2 = player2.shield_size(shield);

            let check_distance = r1 + r2;
            let real_distance = ((x1 - x2).powi(2) + (y1 - y2).powi(2)).sqrt();
            check_distance > real_distance
        } else {
            false
        }
    }
    else {
        false
    }
}

#[allow(dead_code)]
pub enum CollisionResult {
    PhantomDef   (HitBox, HurtBox),
    PhantomAtk   (HitBox, EntityKey),
    HitDef       { hitbox: HitBox, hurtbox: HurtBox, player_atk_i: EntityKey },
    HitAtk       { hitbox: HitBox, player_def_i: EntityKey, point: (f32, f32) },
    HitShieldAtk { hitbox: HitBox, power_shield: Option<PowerShield>, player_def_i: EntityKey },
    HitShieldDef { hitbox: HitBox, power_shield: Option<PowerShield>, player_atk_i: EntityKey },
    ReflectDef   (HitBox), // TODO: add further details required for recreating projectile
    ReflectAtk   (HitBox),
    AbsorbDef    (HitBox),
    AbsorbAtk    (HitBox),
    GrabDef      (EntityKey),
    GrabAtk      (EntityKey),
    Clang        { rebound: bool },
}

// Thoughts on special cases
// *    when one hitbox connects to multiple hurtboxes HitDef is sent to all defenders
// *    when one hurtbox is hit by multiple hitboxes it receives HitDef from all attackers
