use crate::entity::fighters::player::Player;
use crate::entity::{Entities, EntityKey, EntityType};
use crate::entity::components::action_state::ActionState;

use canon_collision_lib::entity_def::{EntityDef, HurtBox, HitBox, CollisionBox, CollisionBoxRole, PowerShield};
use canon_collision_lib::stage::Surface;

use treeflection::KeyedContextVec;
use slotmap::SecondaryMap;

/// returns a list of hit results for each entity
pub fn collision_check(entities: &Entities, entity_definitions: &KeyedContextVec<EntityDef>, surfaces: &[Surface]) -> SecondaryMap<EntityKey, Vec<CollisionResult>> {
    let mut result = SecondaryMap::<EntityKey, Vec<CollisionResult>>::new();
    for key in entities.keys() {
        result.insert(key, vec!());
    }

    'entity_atk: for (entity_atk_i, entity_atk) in entities.iter() {
        let entity_atk_xy = entity_atk.public_bps_xy(entities, entity_definitions, surfaces);
        let entity_atk_def = &entity_definitions[entity_atk.state.entity_def_key.as_ref()];
        let frame_atk = entity_atk.relative_frame(entity_atk_def, surfaces);
        let colboxes_atk = frame_atk.get_hitboxes();
        for (entity_defend_i, entity_defend) in entities.iter() {
            let entity_defend_xy = entity_defend.public_bps_xy(entities, entity_definitions, surfaces);
            if entity_atk_i != entity_defend_i && entity_atk.can_hit(entity_defend) && entity_atk.hitlist().iter().all(|x| *x != entity_defend_i) {
                let entity_defend_def = &entity_definitions[entity_defend.state.entity_def_key.as_ref()];
                let frame_defend = entity_defend.relative_frame(entity_defend_def, surfaces);

                'hitbox_atk: for colbox_atk in &colboxes_atk {
                    if let CollisionBoxRole::Hit (ref hitbox_atk) = colbox_atk.role {
                        if let EntityType::Fighter(fighter) = &entity_defend.ty {
                            let player_defend = fighter.get_player();
                            if colbox_shield_collision_check(entity_atk_xy, colbox_atk, entity_defend_xy, player_defend, entity_defend_def, &entity_defend.state) {
                                result[entity_atk_i].push(CollisionResult::HitShieldAtk {
                                    hitbox: hitbox_atk.clone(),
                                    power_shield: entity_defend_def.power_shield.clone(),
                                    entity_defend_i
                                });
                                result[entity_defend_i].push(CollisionResult::HitShieldDef {
                                    hitbox: hitbox_atk.clone(),
                                    power_shield: entity_defend_def.power_shield.clone(),
                                    entity_atk_i
                                });
                                break 'hitbox_atk;
                            }
                        }

                        if hitbox_atk.enable_clang {
                            for colbox_def in frame_defend.colboxes.iter() {
                                match &colbox_def.role {
                                    // TODO: How do we only run the clang handler once?
                                    &CollisionBoxRole::Hit (ref hitbox_def) => {
                                        if let ColBoxCollisionResult::Hit (point) = colbox_collision_check(entity_atk_xy, colbox_atk, entity_defend_xy, colbox_def) {
                                            let damage_diff = hitbox_atk.damage as i64 - hitbox_def.damage as i64; // TODO: retrieve proper damage with move staling etc

                                            if damage_diff >= 9 {
                                                result[entity_atk_i].push(CollisionResult::Clang { rebound: hitbox_atk.enable_rebound });
                                                result[entity_defend_i].push(CollisionResult::HitAtk { hitbox: hitbox_atk.clone(), entity_defend_i: entity_defend_i, point });
                                            }
                                            else if damage_diff <= -9 {
                                                result[entity_atk_i].push(CollisionResult::HitAtk { hitbox: hitbox_atk.clone(), entity_defend_i: entity_defend_i, point });
                                                result[entity_defend_i].push(CollisionResult::Clang { rebound: hitbox_def.enable_rebound });
                                            }
                                            else {
                                                result[entity_atk_i].push(CollisionResult::Clang { rebound: hitbox_atk.enable_rebound });
                                                result[entity_defend_i].push(CollisionResult::Clang { rebound: hitbox_def.enable_rebound });
                                            }
                                            break 'entity_atk;
                                        }
                                    }
                                    _ => { }
                                }
                            }
                        }

                        for colbox_def in frame_defend.colboxes.iter() {
                            match colbox_collision_check(entity_atk_xy, colbox_atk, entity_defend_xy, colbox_def) {
                                ColBoxCollisionResult::Hit (point) => {
                                    match &colbox_def.role {
                                        &CollisionBoxRole::Hurt (ref hurtbox) => {
                                            result[entity_atk_i].push(CollisionResult::HitAtk { hitbox: hitbox_atk.clone(), entity_defend_i: entity_defend_i, point });
                                            result[entity_defend_i].push(CollisionResult::HitDef { hitbox: hitbox_atk.clone(), hurtbox: hurtbox.clone(), entity_atk_i: entity_atk_i });
                                            break 'entity_atk;
                                        }
                                        &CollisionBoxRole::Invincible => {
                                            result[entity_atk_i].push(CollisionResult::HitAtk { hitbox: hitbox_atk.clone(), entity_defend_i: entity_defend_i, point });
                                            break 'entity_atk;
                                        }
                                        _ => { }
                                    }
                                }
                                ColBoxCollisionResult::Phantom (_) => {
                                    match &colbox_def.role {
                                        &CollisionBoxRole::Hurt (ref hurtbox) => {
                                            result[entity_atk_i].push(CollisionResult::PhantomAtk (hitbox_atk.clone(), entity_defend_i));
                                            result[entity_defend_i].push(CollisionResult::PhantomDef (hitbox_atk.clone(), hurtbox.clone()));
                                            break 'entity_atk;
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
                            for colbox_def in &frame_defend.colboxes[..] {
                                if let ColBoxCollisionResult::Hit (_) = colbox_collision_check(entity_atk_xy, colbox_atk, entity_defend_xy, colbox_def) {
                                    result[entity_atk_i].push(CollisionResult::GrabAtk (entity_defend_i));
                                    result[entity_defend_i].push(CollisionResult::GrabDef (entity_atk_i));
                                    break 'entity_atk;
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

fn colbox_shield_collision_check(player1_xy: (f32, f32), colbox1: &CollisionBox,  player2_xy: (f32, f32), player2: &Player, fighter2: &EntityDef, player2_state: &ActionState) -> bool {
    if let &Some(ref shield) = &fighter2.shield {
        if player2.is_shielding(player2_state) {
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
    HitDef       { hitbox: HitBox, hurtbox: HurtBox, entity_atk_i: EntityKey },
    HitAtk       { hitbox: HitBox, entity_defend_i: EntityKey, point: (f32, f32) },
    HitShieldAtk { hitbox: HitBox, power_shield: Option<PowerShield>, entity_defend_i: EntityKey },
    HitShieldDef { hitbox: HitBox, power_shield: Option<PowerShield>, entity_atk_i: EntityKey },
    ReflectDef   (HitBox),
    ReflectAtk   { hitbox: HitBox, entity_def_i: EntityKey },
    AbsorbDef    (HitBox),
    AbsorbAtk    (HitBox),
    GrabDef      (EntityKey),
    GrabAtk      (EntityKey),
    Clang        { rebound: bool },
}

// Thoughts on special cases
// *    when one hitbox connects to multiple hurtboxes HitDef is sent to all defenders
// *    when one hurtbox is hit by multiple hitboxes it receives HitDef from all attackers
