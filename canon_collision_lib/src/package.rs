use std::collections::HashSet;
use std::fs;
use std::fs::File;

use std::path::{Path, PathBuf};

use treeflection::{KeyedContextVec, Node, NodeRunner, NodeToken};

use crate::entity_def::{ActionFrame, CollisionBox, CollisionBoxRole, EntityDef, EntityDefType};
use crate::files;
use crate::stage::Stage;

/// Stores persistent that data that can be modified at runtime.
#[derive(Clone, Serialize, Deserialize)]
pub struct Package {
    pub stages: KeyedContextVec<Stage>, // TODO: Can just use a std map here
    pub entities: KeyedContextVec<EntityDef>,
    path: PathBuf,
    package_updates: Vec<PackageUpdate>,
}

impl Default for Package {
    fn default() -> Package {
        panic!("Why would you do that >.>");
    }
}

impl Package {
    pub fn has_updates(&self) -> bool {
        !self.package_updates.is_empty()
    }

    /// Loads and returns the package with the specified name.
    /// Returns None if the package doesnt exist or is broken.
    pub fn open(path: PathBuf) -> Option<Package> {
        let mut package = Package {
            path,
            stages: KeyedContextVec::new(),
            entities: KeyedContextVec::new(),
            package_updates: vec![],
        };

        if package.load().is_ok() {
            Some(package)
        } else {
            None
        }
    }

    pub fn find_package_in_parent_dirs() -> Option<PathBuf> {
        let path = std::env::current_dir().unwrap();
        Package::find_package_in_parent_dirs_core(&path)
    }

    fn find_package_in_parent_dirs_core(path: &Path) -> Option<PathBuf> {
        let package_path = path.join("package");
        match fs::metadata(&package_path) {
            Ok(_) => Some(package_path),
            Err(_) => Package::find_package_in_parent_dirs_core(path.parent()?),
        }
    }

    pub fn generate_base(path: PathBuf) -> Package {
        let mut package = Package {
            path,
            stages: KeyedContextVec::from_vec(vec![(
                String::from("base_stage.cbor"),
                Stage::default(),
            )]),
            entities: KeyedContextVec::from_vec(vec![(
                String::from("base_fighter.cbor"),
                EntityDef::default(),
            )]),
            package_updates: vec![],
        };
        package.save();
        package.load().unwrap();
        package
    }

    // Write to a new folder first, in case there is a panic in between deleting and writing data.
    // Then we delete the existing folder and rename the new one
    pub fn save(&mut self) -> String {
        // setup new directory to save to
        let new_path = self.path.with_file_name("package_temp_path");
        if let Err(_) = fs::create_dir(&new_path) {
            if let Err(_) = fs::remove_dir_all(&new_path) {
                return String::from("Save FAILED! Failed to delete an existing *_name_conflict_avoiding_temp_string folder");
            }
            if let Err(_) = fs::create_dir(&new_path) {
                return String::from("Save FAILED! Failed to create *_name_conflict_avoiding_temp_string folder even after succesfully deleting an existing one");
            }
        }

        // save all cbor files
        for (key, fighter) in self.entities.key_value_iter() {
            files::save_struct_cbor(&new_path.join("Entities").join(key), fighter);
        }

        for (key, stage) in self.stages.key_value_iter() {
            files::save_struct_cbor(&new_path.join("Stages").join(key), stage);
        }

        // replace old directory with new directory
        fs::remove_dir_all(&self.path).ok();
        if let Err(_) = fs::rename(new_path, &self.path) {
            return String::from("Save FAILED! Failed to rename temp package");
        }

        String::from("Save completed successfully.")
    }

    pub fn load(&mut self) -> Result<(), String> {
        let mut entities = vec![];
        if let Ok(dir) = fs::read_dir(self.path.join("Entities")) {
            for path in dir {
                let full_path = path.unwrap().path();
                let key = full_path.file_name().unwrap().to_str().unwrap().to_string();

                let reader = File::open(full_path).map_err(|x| format!("{:?}", x))?;
                let mut entity: EntityDef =
                    serde_cbor::from_reader(reader).map_err(|x| format!("{:?}", x))?;
                entity.cleanup();
                entities.push((key, entity));
            }
        }
        entities.sort_by_key(|x| x.0.clone());
        self.entities = KeyedContextVec::from_vec(entities);

        let mut stages = vec![];
        if let Ok(dir) = fs::read_dir(self.path.join("Stages")) {
            for path in dir {
                let full_path = path.unwrap().path();
                let key = full_path.file_name().unwrap().to_str().unwrap().to_string();

                let reader = File::open(full_path).map_err(|x| format!("{:?}", x))?;
                let stage = serde_cbor::from_reader(reader).map_err(|x| format!("{:?}", x))?;
                stages.push((key, stage));
            }
        }
        stages.sort_by_key(|x| x.0.clone());
        self.stages = KeyedContextVec::from_vec(stages);

        self.force_update_entire_package();
        Ok(())
    }

    pub fn new_fighter_frame(&mut self, fighter: &str, action: &str, frame: usize) {
        let new_frame = {
            let action_frames = &self.entities[fighter].actions[action].frames;
            action_frames[frame].clone()
        };
        self.insert_fighter_frame(fighter, action, frame, new_frame);
    }

    pub fn insert_fighter_frame(
        &mut self,
        fighter: &str,
        action: &str,
        frame: usize,
        action_frame: ActionFrame,
    ) {
        let action_frames = &mut self.entities[fighter].actions[action].frames;

        action_frames.insert(frame, action_frame.clone());

        self.package_updates
            .push(PackageUpdate::InsertFighterFrame {
                fighter: fighter.to_string(),
                action: action.to_string(),
                frame_index: frame,
                frame: action_frame,
            });
    }

    pub fn delete_fighter_frame(&mut self, fighter: &str, action: &str, frame: usize) -> bool {
        let action_frames = &mut self.entities[fighter].actions[action].frames;

        // there must always be at least one frame and only delete frames that exist
        if action_frames.len() > 1 && frame < action_frames.len() {
            action_frames.remove(frame);

            self.package_updates
                .push(PackageUpdate::DeleteFighterFrame {
                    fighter: fighter.to_string(),
                    action: action.to_string(),
                    frame_index: frame,
                });
            true
        } else {
            false
        }
    }

    /// add the passed collisionbox to the specified fighter frame
    /// the added collisionbox is linked to the specified collisionboxes
    /// returns the index the collisionbox was added to.
    pub fn append_fighter_colbox(
        &mut self,
        fighter: &str,
        action: &str,
        frame: usize,
        new_colbox: CollisionBox,
    ) -> usize {
        let fighter_frame = &mut self.entities[fighter].actions[action].frames[frame];
        let new_colbox_index = fighter_frame.colboxes.len();
        fighter_frame.colboxes.push(new_colbox);

        self.package_updates
            .push(PackageUpdate::DeleteFighterFrame {
                fighter: fighter.to_string(),
                action: action.to_string(),
                frame_index: frame,
            });
        self.package_updates
            .push(PackageUpdate::InsertFighterFrame {
                fighter: fighter.to_string(),
                action: action.to_string(),
                frame_index: frame,
                frame: fighter_frame.clone(),
            });

        new_colbox_index
    }

    pub fn delete_fighter_colboxes(
        &mut self,
        fighter: &str,
        action: &str,
        frame: usize,
        colboxes_to_delete: &HashSet<usize>,
    ) {
        let fighter_frame = &mut self.entities[fighter].actions[action].frames[frame];
        {
            let colboxes = &mut fighter_frame.colboxes;

            // ensure that collisionboxes are deleted in an order in which the indexes continue to refer to the same element.
            let mut colboxes_to_delete = colboxes_to_delete.iter().collect::<Vec<_>>();
            colboxes_to_delete.sort();
            colboxes_to_delete.reverse();

            for delete_colbox_i in colboxes_to_delete {
                // delete colboxes
                let delete_colbox_i = *delete_colbox_i;
                colboxes.remove(delete_colbox_i);
            }
        }

        self.package_updates
            .push(PackageUpdate::DeleteFighterFrame {
                fighter: fighter.to_string(),
                action: action.to_string(),
                frame_index: frame,
            });
        self.package_updates
            .push(PackageUpdate::InsertFighterFrame {
                fighter: fighter.to_string(),
                action: action.to_string(),
                frame_index: frame,
                frame: fighter_frame.clone(),
            });
    }

    pub fn move_fighter_colboxes(
        &mut self,
        fighter: &str,
        action: &str,
        frame: usize,
        moved_colboxes: &HashSet<usize>,
        distance: (f32, f32),
    ) {
        let fighter_frame = &mut self.entities[fighter].actions[action].frames[frame];
        {
            let colboxes = &mut fighter_frame.colboxes;
            let (d_x, d_y) = distance;

            for i in moved_colboxes {
                let (b_x, b_y) = colboxes[*i].point;
                colboxes[*i].point = (b_x + d_x, b_y + d_y);
            }
        }

        self.package_updates
            .push(PackageUpdate::DeleteFighterFrame {
                fighter: fighter.to_string(),
                action: action.to_string(),
                frame_index: frame,
            });
        self.package_updates
            .push(PackageUpdate::InsertFighterFrame {
                fighter: fighter.to_string(),
                action: action.to_string(),
                frame_index: frame,
                frame: fighter_frame.clone(),
            });
    }

    pub fn point_hitbox_angles_to(
        &mut self,
        fighter: &str,
        action: &str,
        frame: usize,
        set_hitboxes: &HashSet<usize>,
        x: f32,
        y: f32,
    ) {
        let colboxes = &mut self.entities[fighter].actions[action].frames[frame].colboxes;
        for i in set_hitboxes {
            let colbox = &mut colboxes[*i];
            if let &mut CollisionBoxRole::Hit(ref mut hitbox) = &mut colbox.role {
                let angle = (y - colbox.point.1).atan2(x - colbox.point.0);
                hitbox.angle = angle.to_degrees();
            }
        }
    }

    pub fn resize_fighter_colboxes(
        &mut self,
        fighter: &str,
        action: &str,
        frame: usize,
        resized_colboxes: &HashSet<usize>,
        size_diff: f32,
    ) {
        let fighter_frame = &mut self.entities[fighter].actions[action].frames[frame];
        {
            let colboxes = &mut fighter_frame.colboxes;

            for i in resized_colboxes {
                let colbox = &mut colboxes[*i];
                colbox.radius += size_diff;
            }
        }

        self.package_updates
            .push(PackageUpdate::DeleteFighterFrame {
                fighter: fighter.to_string(),
                action: action.to_string(),
                frame_index: frame,
            });
        self.package_updates
            .push(PackageUpdate::InsertFighterFrame {
                fighter: fighter.to_string(),
                action: action.to_string(),
                frame_index: frame,
                frame: fighter_frame.clone(),
            });
    }

    /// All colboxes or links containing colboxes from reordered_colboxes are sent to the front
    pub fn fighter_colboxes_order_increase(
        &mut self,
        fighter: &str,
        action: &str,
        frame: usize,
        reordered_colboxes: &HashSet<usize>,
    ) {
        let fighter_frame = &mut self.entities[fighter].actions[action].frames[frame];
        {
            let mut reordered_colboxes: Vec<usize> = reordered_colboxes.iter().cloned().collect();
            reordered_colboxes.sort_unstable();
            reordered_colboxes.reverse();
            for i in reordered_colboxes {
                let colbox = fighter_frame.colboxes.remove(i);
                fighter_frame.colboxes.insert(i, colbox);
            }
        }

        self.package_updates
            .push(PackageUpdate::DeleteFighterFrame {
                fighter: fighter.to_string(),
                action: action.to_string(),
                frame_index: frame,
            });
        self.package_updates
            .push(PackageUpdate::InsertFighterFrame {
                fighter: fighter.to_string(),
                action: action.to_string(),
                frame_index: frame,
                frame: fighter_frame.clone(),
            });
    }

    /// All colboxes or links containing colboxes from reordered_colboxes are sent to the back
    pub fn fighter_colboxes_order_set_last(
        &mut self,
        fighter: &str,
        action: &str,
        frame: usize,
        reordered_colboxes: &HashSet<usize>,
    ) {
        let fighter_frame = &mut self.entities[fighter].actions[action].frames[frame];
        {
            let mut reordered_colboxes: Vec<usize> = reordered_colboxes.iter().cloned().collect();
            reordered_colboxes.sort_unstable();
            for i in reordered_colboxes {
                let colbox = fighter_frame.colboxes.remove(i);
                fighter_frame.colboxes.insert(0, colbox);
            }
        }

        self.package_updates
            .push(PackageUpdate::DeleteFighterFrame {
                fighter: fighter.to_string(),
                action: action.to_string(),
                frame_index: frame,
            });
        self.package_updates
            .push(PackageUpdate::InsertFighterFrame {
                fighter: fighter.to_string(),
                action: action.to_string(),
                frame_index: frame,
                frame: fighter_frame.clone(),
            });
    }

    /// All colboxes or links containing colboxes from reordered_colboxes are sent to the front
    pub fn fighter_colboxes_order_decrease(
        &mut self,
        fighter: &str,
        action: &str,
        frame: usize,
        reordered_colboxes: &HashSet<usize>,
    ) {
        let fighter_frame = &mut self.entities[fighter].actions[action].frames[frame];
        {
            let mut reordered_colboxes: Vec<usize> = reordered_colboxes.iter().cloned().collect();
            reordered_colboxes.sort_unstable();
            for i in reordered_colboxes {
                let colbox = fighter_frame.colboxes.remove(i);
                fighter_frame.colboxes.insert(i, colbox);
            }
        }

        self.package_updates
            .push(PackageUpdate::DeleteFighterFrame {
                fighter: fighter.to_string(),
                action: action.to_string(),
                frame_index: frame,
            });
        self.package_updates
            .push(PackageUpdate::InsertFighterFrame {
                fighter: fighter.to_string(),
                action: action.to_string(),
                frame_index: frame,
                frame: fighter_frame.clone(),
            });
    }

    /// All colboxes or links containing colboxes from reordered_colboxes are sent to the front
    pub fn fighter_colboxes_order_set_first(
        &mut self,
        fighter: &str,
        action: &str,
        frame: usize,
        reordered_colboxes: &HashSet<usize>,
    ) {
        let fighter_frame = &mut self.entities[fighter].actions[action].frames[frame];
        {
            let mut reordered_colboxes: Vec<usize> = reordered_colboxes.iter().cloned().collect();
            reordered_colboxes.sort_unstable();
            reordered_colboxes.reverse();
            for i in reordered_colboxes {
                let colbox = fighter_frame.colboxes.remove(i);
                fighter_frame.colboxes.insert(0, colbox);
            }
        }

        self.package_updates
            .push(PackageUpdate::DeleteFighterFrame {
                fighter: fighter.to_string(),
                action: action.to_string(),
                frame_index: frame,
            });
        self.package_updates
            .push(PackageUpdate::InsertFighterFrame {
                fighter: fighter.to_string(),
                action: action.to_string(),
                frame_index: frame,
                frame: fighter_frame.clone(),
            });
    }

    // TODO: Refactor to use a reference would be way faster
    pub fn force_update_entire_package(&mut self) {
        let package_update = PackageUpdate::Package(self.clone());
        self.package_updates.push(package_update);
    }

    pub fn updates(&mut self) -> Vec<PackageUpdate> {
        std::mem::take(&mut self.package_updates)
    }

    pub fn fighters(&self) -> Vec<(String, &EntityDef)> {
        let mut result = vec![];
        for (key, entity) in self.entities.key_value_iter() {
            if let EntityDefType::Fighter(_) = entity.ty {
                result.push((key.clone(), entity));
            }
        }
        result.sort_by_key(|x| x.0.clone());
        result
    }
}

impl Node for Package {
    fn node_step(&mut self, mut runner: NodeRunner) -> String {
        let result = match runner.step() {
            NodeToken::ChainProperty(property) => match property.as_str() {
                "entities" => self.entities.node_step(runner),
                "stages" => self.stages.node_step(runner),
                prop => format!("Package does not have a property '{}'", prop),
            },
            NodeToken::Help => String::from(
                r#"
Package Help

Commands:
*   help    - display this help
*   save    - save changes to disc
*   reload  - reload from disc, all changes are lost

Accessors:
*   .entities - KeyedContextVec
*   .stages   - KeyedContextVec"#,
            ),
            NodeToken::Custom(action, _) => match action.as_ref() {
                "save" => self.save(),
                "reload" => {
                    if let Err(err) = self.load() {
                        err
                    } else {
                        String::from("Reload completed successfully.")
                    }
                }
                _ => {
                    format!("Package cannot '{}'", action)
                }
            },
            action => {
                format!("Package cannot '{:?}'", action)
            }
        };

        self.force_update_entire_package();
        result
    }
}

// Finer grained changes are used when speed is needed
#[derive(Clone, Serialize, Deserialize)]
pub enum PackageUpdate {
    Package(Package),
    DeleteFighterFrame {
        fighter: String,
        action: String,
        frame_index: usize,
    },
    InsertFighterFrame {
        fighter: String,
        action: String,
        frame_index: usize,
        frame: ActionFrame,
    },
    DeleteStage {
        index: usize,
        key: String,
    },
    InsertStage {
        index: usize,
        key: String,
        stage: Stage,
    },
}
