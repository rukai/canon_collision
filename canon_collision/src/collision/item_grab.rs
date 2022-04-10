use crate::entity::{Entities, EntityKey, EntityType};

use canon_collision_lib::entity_def::EntityDef;
use canon_collision_lib::stage::Surface;

use slotmap::SecondaryMap;
use treeflection::KeyedContextVec;

/// Logic:
/// 1. Checks for collisions between players item_grab_box and items item_grab_box
/// 2. Assign each player its closest colliding item, unless another player is already closer to that item.
/// 3. Repeat until stable (10x max, in case the distances are equal or something)
pub fn collision_check(
    entities: &Entities,
    entity_definitions: &KeyedContextVec<EntityDef>,
    surfaces: &[Surface],
) -> SecondaryMap<EntityKey, EntityKey> {
    let mut player_grabs = SecondaryMap::<EntityKey, PlayerGrabHit>::new();

    let mut player_grabs_last_len = 0;
    for _ in 0..10 {
        for (player_i, entity_player) in entities.iter() {
            if let EntityType::Fighter(fighter) = &entity_player.ty {
                let player = fighter.get_player();
                if player.get_held_item(entities).is_none() {
                    let (player_x, player_y) =
                        entity_player.public_bps_xy(entities, entity_definitions, surfaces);
                    let player_item_grab_box =
                        entity_player.item_grab_box(entities, entity_definitions, surfaces);
                    'entity_check: for (item_i, entity_item) in entities.iter() {
                        if let EntityType::Item(item) = &entity_item.ty {
                            if !item.body.is_grabbed() {
                                let (item_x, item_y) = entity_item.public_bps_xy(
                                    entities,
                                    entity_definitions,
                                    surfaces,
                                );
                                let item_item_grab_box = entity_item.item_grab_box(
                                    entities,
                                    entity_definitions,
                                    surfaces,
                                );
                                let collision = match (&player_item_grab_box, &item_item_grab_box) {
                                    (Some(a), Some(b)) => a.collision(b),
                                    _ => false,
                                };
                                if collision {
                                    // TODO: we probably want to check overlap of item_grab_box's rather then comparing bps_xy
                                    let distance = ((item_x - player_x).powi(2)
                                        + (item_y - player_y).powi(2))
                                    .sqrt();

                                    let shortest_item = player_grabs
                                        .get(item_i)
                                        .map(|x| distance < x.distance)
                                        .unwrap_or(true);
                                    if shortest_item {
                                        let mut to_delete = None;
                                        for (items_player_i, hit) in player_grabs.iter() {
                                            if hit.item_i == item_i {
                                                if distance < hit.distance {
                                                    to_delete = Some(items_player_i);
                                                } else {
                                                    continue 'entity_check;
                                                }
                                            }
                                        }
                                        if let Some(i) = to_delete {
                                            player_grabs.remove(i);
                                        }

                                        player_grabs
                                            .insert(player_i, PlayerGrabHit { item_i, distance });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if player_grabs_last_len == player_grabs.len() {
            break;
        }
        player_grabs_last_len = player_grabs.len();
    }

    let mut result = SecondaryMap::<EntityKey, EntityKey>::new();
    for (player_i, hit) in player_grabs.iter() {
        result.insert(player_i, hit.item_i);
        result.insert(hit.item_i, player_i);
    }

    result
}

#[derive(Debug)]
struct PlayerGrabHit {
    item_i: EntityKey,
    distance: f32,
}
