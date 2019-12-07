use std::collections::{HashSet, HashMap};
use std::fs;
use std::mem;
use std::fs::File;
use std::path::{Path, PathBuf};

use sha2::{Sha256, Digest};
use serde_json;
use treeflection::{Node, NodeRunner, NodeToken, KeyedContextVec};

use crate::fighter::{Fighter, ActionFrame, CollisionBox, CollisionBoxRole};
use crate::files::{self, engine_version};
use crate::rules::Rules;
use crate::stage::Stage;

/// Stores persistent that data that can be modified at runtime.
#[derive(Clone, Serialize, Deserialize)]
pub struct Package {
    pub meta:            PackageMeta,
    pub rules:           Rules,
    pub stages:          KeyedContextVec<Stage>, // TODO: Can just use a std map here
    pub fighters:        KeyedContextVec<Fighter>,
        path:            PathBuf,
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
            meta:            PackageMeta::new(),
            rules:           Rules::default(),
            stages:          KeyedContextVec::new(),
            fighters:        KeyedContextVec::new(),
            package_updates: vec!(),
        };

        if let Ok(_) = package.load() {
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
            Ok(_) => {
                Some(package_path.to_path_buf())
            }
            Err(_) => {
                Package::find_package_in_parent_dirs_core(path.parent()?)
            }
        }
    }

    pub fn generate_base(path: PathBuf) -> Package {
        let meta = PackageMeta {
            engine_version:    engine_version(),
            published_version: 0,
            published:         false,
            hash:              "".to_string(),
            fighter_keys:      vec!(),
            stage_keys:        vec!(),
        };

        let mut package = Package {
            path,
            meta:            meta,
            rules:           Rules::default(),
            stages:          KeyedContextVec::from_vec(vec!((String::from("base_stage.cbor"), Stage::default()))),
            fighters:        KeyedContextVec::from_vec(vec!((String::from("base_fighter.cbor"), Fighter::default()))),
            package_updates: vec!(),
        };
        package.save();
        package.load().unwrap();
        package
    }

    // Write to a new folder first, in case there is a panic in between deleting and writing data.
    // Then we delete the existing folder and rename the new one
    pub fn save(&mut self) -> String {
        if self.meta.published {
            return String::from("Save FAILED! The published property in package_meta is set.");
        }

        self.meta.fighter_keys = self.fighters.keys();
        self.meta.stage_keys = self.stages.keys();
        self.meta.hash = self.compute_hash();

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
        files::save_struct_cbor(new_path.join("rules.cbor"), &self.rules);
        files::save_struct_cbor(new_path.join("package_meta.cbor"), &self.meta);

        for (key, fighter) in self.fighters.key_value_iter() {
            files::save_struct_cbor(new_path.join("Fighters").join(key), fighter);
        }
        
        for (key, stage) in self.stages.key_value_iter() {
            files::save_struct_cbor(new_path.join("Stages").join(key), stage);
        }

        // replace old directory with new directory
        fs::remove_dir_all(&self.path).ok();
        if let Err(_) = fs::rename(new_path, &self.path) {
            return String::from("Save FAILED! Failed to rename temp package");
        }

        String::from("Save completed successfully.")
    }

    pub fn load(&mut self) -> Result<(), String> {
        // load the meta file if exists otherwise generate one.
        // if the meta file exists but is invalid fail the package load
        self.meta = match File::open(self.path.join("package_meta.json")) {
            Ok (reader) => {
                serde_cbor::from_reader(reader).map_err(|x| format!("{:?}", x))?
            }
            Err (_) => PackageMeta::default()
        };

        // load the rules file if exists otherwise generate one.
        // if the rules file exists but is invalid fail the package load
        self.rules = match File::open(self.path.join("rules.json")) {
            Ok (reader) => {
                serde_cbor::from_reader(reader).map_err(|x| format!("{:?}", x))?
            }
            Err (_) => Rules::default()
        };

        // Get paths to the fighters
        let mut fighter_paths: HashMap<String, PathBuf> = HashMap::new();
        if let Ok (dir) = fs::read_dir(self.path.join("Fighters")) {
            for path in dir {
                let full_path = path.unwrap().path();
                let key = full_path.file_name().unwrap().to_str().unwrap().to_string();
                fighter_paths.insert(key, full_path);
            }
        }

        // Use meta.fighter_keys for fighter ordering
        self.fighters = KeyedContextVec::new();
        for file_name in &self.meta.fighter_keys {
            if let Some(file_path) = fighter_paths.remove(file_name) {
                let reader = File::open(file_path).map_err(|x| format!("{:?}", x))?;
                let fighter = serde_cbor::from_reader(reader).map_err(|x| format!("{:?}", x))?;
                self.fighters.push(file_name.clone(), fighter);
            }
        }

        // add remaining fighters in any order
        for (file_name, file_path) in fighter_paths {
            let reader = File::open(file_path).map_err(|x| format!("{:?}", x))?;
            let fighter = serde_cbor::from_reader(reader).map_err(|x| format!("{:?}", x))?;
            self.fighters.push(file_name.clone(), fighter);
        }

        // Get paths to the stages
        let mut stage_paths: HashMap<String, PathBuf> = HashMap::new();
        if let Ok (dir) = fs::read_dir(self.path.join("Stages")) {
            for path in dir {
                let full_path = path.unwrap().path();
                let key = full_path.file_name().unwrap().to_str().unwrap().to_string();
                stage_paths.insert(key, full_path);
            }
        }

        // Use meta.stage_keys for stage ordering
        self.stages = KeyedContextVec::new();
        for file_name in &self.meta.stage_keys {
            if let Some(file_path) = stage_paths.remove(file_name) {
                let reader = File::open(file_path).map_err(|x| format!("{:?}", x))?;
                let stage = serde_cbor::from_reader(reader).map_err(|x| format!("{:?}", x))?;
                self.stages.push(file_name.clone(), stage);
            }
        }

        // add remaining stages in any order
        for (file_name, file_path) in stage_paths {
            let reader = File::open(file_path).map_err(|x| format!("{:?}", x))?;
            let stage = serde_cbor::from_reader(reader).map_err(|x| format!("{:?}", x))?;
            self.stages.push(file_name.clone(), stage);
        }

        self.force_update_entire_package();
        Ok(())
    }

    pub fn compute_hash(&self) -> String {
        let mut hasher = Sha256::default();
        hasher.input(&serde_json::to_vec(&self.rules).unwrap());

        for stage in self.stages.iter() {
            hasher.input(&serde_json::to_vec(stage).unwrap());
        }

        for fighter in self.fighters.iter() {
            hasher.input(&serde_json::to_vec(fighter).unwrap());
        }

        hasher.result().iter().map(|x| format!("{:x}", x)).collect()
    }

    pub fn new_fighter_frame(&mut self, fighter: &str, action: usize, frame: usize) {
        let new_frame = {
            let action_frames = &self.fighters[fighter].actions[action].frames;
            action_frames[frame].clone()
        };
        self.insert_fighter_frame(fighter, action, frame, new_frame);
    }

    pub fn insert_fighter_frame(&mut self, fighter: &str, action: usize, frame: usize, action_frame: ActionFrame) {
        let action_frames = &mut self.fighters[fighter].actions[action].frames;

        action_frames.insert(frame, action_frame.clone());

        self.package_updates.push(PackageUpdate::InsertFighterFrame {
            fighter:     fighter.to_string(),
            action:      action,
            frame_index: frame,
            frame:       action_frame,
        });
    }

    pub fn delete_fighter_frame(&mut self, fighter: &str, action: usize, frame: usize) -> bool {
        let action_frames = &mut self.fighters[fighter].actions[action].frames;

        if action_frames.len() > 1 {
            action_frames.remove(frame);

            self.package_updates.push(PackageUpdate::DeleteFighterFrame {
                fighter:     fighter.to_string(),
                action:      action,
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
        &mut self, fighter: &str, action: usize, frame: usize, new_colbox: CollisionBox
    ) -> usize {
        let fighter_frame = &mut self.fighters[fighter].actions[action].frames[frame];
        let new_colbox_index = fighter_frame.colboxes.len();
        fighter_frame.colboxes.push(new_colbox);

        self.package_updates.push(PackageUpdate::DeleteFighterFrame {
            fighter:     fighter.to_string(),
            action:      action,
            frame_index: frame,
        });
        self.package_updates.push(PackageUpdate::InsertFighterFrame {
            fighter:     fighter.to_string(),
            action:      action,
            frame_index: frame,
            frame:       fighter_frame.clone(),
        });

        new_colbox_index
    }

    pub fn delete_fighter_colboxes(&mut self, fighter: &str, action: usize, frame: usize, colboxes_to_delete: &HashSet<usize>) {
        let fighter_frame = &mut self.fighters[fighter].actions[action].frames[frame];
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

        self.package_updates.push(PackageUpdate::DeleteFighterFrame {
            fighter:     fighter.to_string(),
            action:      action,
            frame_index: frame,
        });
        self.package_updates.push(PackageUpdate::InsertFighterFrame {
            fighter:     fighter.to_string(),
            action:      action,
            frame_index: frame,
            frame:       fighter_frame.clone(),
        });
    }

    pub fn move_fighter_colboxes(&mut self, fighter: &str, action: usize, frame: usize, moved_colboxes: &HashSet<usize>, distance: (f32, f32)) {
        let fighter_frame = &mut self.fighters[fighter].actions[action].frames[frame];
        {
            let colboxes = &mut fighter_frame.colboxes;
            let (d_x, d_y) = distance;

            for i in moved_colboxes {
                let (b_x, b_y) = colboxes[*i].point;
                colboxes[*i].point = (b_x + d_x, b_y + d_y);
            }
        }

        self.package_updates.push(PackageUpdate::DeleteFighterFrame {
            fighter:     fighter.to_string(),
            action:      action,
            frame_index: frame,
        });
        self.package_updates.push(PackageUpdate::InsertFighterFrame {
            fighter:     fighter.to_string(),
            action:      action,
            frame_index: frame,
            frame:       fighter_frame.clone(),
        });
    }

    pub fn point_hitbox_angles_to(&mut self, fighter: &str, action: usize, frame: usize, set_hitboxes: &HashSet<usize>, x: f32, y: f32) {
        let colboxes = &mut self.fighters[fighter].actions[action].frames[frame].colboxes;
        for i in set_hitboxes {
            let colbox = &mut colboxes[*i];
            if let &mut CollisionBoxRole::Hit(ref mut hitbox) = &mut colbox.role {
                let angle = (y - colbox.point.1).atan2(x - colbox.point.0);
                hitbox.angle = angle.to_degrees();
            }
        }
    }

    pub fn resize_fighter_colboxes(&mut self, fighter: &str, action: usize, frame: usize, resized_colboxes: &HashSet<usize>, size_diff: f32) {
        let fighter_frame = &mut self.fighters[fighter].actions[action].frames[frame];
        {
            let colboxes = &mut fighter_frame.colboxes;

            for i in resized_colboxes {
                let colbox = &mut colboxes[*i];
                colbox.radius += size_diff;
            }
        }

        self.package_updates.push(PackageUpdate::DeleteFighterFrame {
            fighter:     fighter.to_string(),
            action:      action,
            frame_index: frame,
        });
        self.package_updates.push(PackageUpdate::InsertFighterFrame {
            fighter:     fighter.to_string(),
            action:      action,
            frame_index: frame,
            frame:       fighter_frame.clone(),
        });
    }

    /// All colboxes or links containing colboxes from reordered_colboxes are sent to the front
    pub fn fighter_colboxes_order_increase(&mut self, fighter: &str, action: usize, frame: usize, reordered_colboxes: &HashSet<usize>) {
        let fighter_frame = &mut self.fighters[fighter].actions[action].frames[frame];
        {
            let mut reordered_colboxes: Vec<usize> = reordered_colboxes.iter().cloned().collect();
            reordered_colboxes.sort();
            reordered_colboxes.reverse();
            for i in reordered_colboxes {
                let colbox = fighter_frame.colboxes.remove(i);
                fighter_frame.colboxes.insert(i, colbox);
            }
        }

        self.package_updates.push(PackageUpdate::DeleteFighterFrame {
            fighter:     fighter.to_string(),
            action:      action,
            frame_index: frame,
        });
        self.package_updates.push(PackageUpdate::InsertFighterFrame {
            fighter:     fighter.to_string(),
            action:      action,
            frame_index: frame,
            frame:       fighter_frame.clone(),
        });
    }

    /// All colboxes or links containing colboxes from reordered_colboxes are sent to the back
    pub fn fighter_colboxes_order_set_last(&mut self, fighter: &str, action: usize, frame: usize, reordered_colboxes: &HashSet<usize>) {
        let fighter_frame = &mut self.fighters[fighter].actions[action].frames[frame];
        {
            let mut reordered_colboxes: Vec<usize> = reordered_colboxes.iter().cloned().collect();
            reordered_colboxes.sort();
            for i in reordered_colboxes {
                let colbox = fighter_frame.colboxes.remove(i);
                fighter_frame.colboxes.insert(0, colbox);
            }
        }

        self.package_updates.push(PackageUpdate::DeleteFighterFrame {
            fighter:     fighter.to_string(),
            action:      action,
            frame_index: frame,
        });
        self.package_updates.push(PackageUpdate::InsertFighterFrame {
            fighter:     fighter.to_string(),
            action:      action,
            frame_index: frame,
            frame:       fighter_frame.clone(),
        });
    }

    /// All colboxes or links containing colboxes from reordered_colboxes are sent to the front
    pub fn fighter_colboxes_order_decrease(&mut self, fighter: &str, action: usize, frame: usize, reordered_colboxes: &HashSet<usize>) {
        let fighter_frame = &mut self.fighters[fighter].actions[action].frames[frame];
        {
            let mut reordered_colboxes: Vec<usize> = reordered_colboxes.iter().cloned().collect();
            reordered_colboxes.sort();
            for i in reordered_colboxes {
                let colbox = fighter_frame.colboxes.remove(i);
                fighter_frame.colboxes.insert(i, colbox);
            }
        }

        self.package_updates.push(PackageUpdate::DeleteFighterFrame {
            fighter:     fighter.to_string(),
            action:      action,
            frame_index: frame,
        });
        self.package_updates.push(PackageUpdate::InsertFighterFrame {
            fighter:     fighter.to_string(),
            action:      action,
            frame_index: frame,
            frame:       fighter_frame.clone(),
        });
    }

    /// All colboxes or links containing colboxes from reordered_colboxes are sent to the front
    pub fn fighter_colboxes_order_set_first(&mut self, fighter: &str, action: usize, frame: usize, reordered_colboxes: &HashSet<usize>) {
        let fighter_frame = &mut self.fighters[fighter].actions[action].frames[frame];
        {
            let mut reordered_colboxes: Vec<usize> = reordered_colboxes.iter().cloned().collect();
            reordered_colboxes.sort();
            reordered_colboxes.reverse();
            for i in reordered_colboxes {
                let colbox = fighter_frame.colboxes.remove(i);
                fighter_frame.colboxes.insert(0, colbox);
            }
        }

        self.package_updates.push(PackageUpdate::DeleteFighterFrame {
            fighter:     fighter.to_string(),
            action:      action,
            frame_index: frame,
        });
        self.package_updates.push(PackageUpdate::InsertFighterFrame {
            fighter:     fighter.to_string(),
            action:      action,
            frame_index: frame,
            frame:       fighter_frame.clone(),
        });
    }

    // TODO: Refactor to use a reference would be way faster
    pub fn force_update_entire_package(&mut self) {
        let package_update = PackageUpdate::Package(self.clone());
        self.package_updates.push(package_update);
    }

    pub fn updates(&mut self) -> Vec<PackageUpdate> {
        mem::replace(&mut self.package_updates, vec!())
    }
}

impl Node for Package {
    fn node_step(&mut self, mut runner: NodeRunner) -> String {
        let result = match runner.step() {
            NodeToken::ChainProperty (property) => {
                match property.as_str() {
                    "fighters" => { self.fighters.node_step(runner) }
                    "stages"   => { self.stages.node_step(runner) }
                    "meta"     => { self.meta.node_step(runner) }
                    "rules"    => { self.rules.node_step(runner) }
                    prop       => format!("Package does not have a property '{}'", prop)
                }
            }
            NodeToken::Help => {
                String::from(r#"
Package Help

Commands:
*   help    - display this help
*   save    - save changes to disc
*   reload  - reload from disc, all changes are lost

Accessors:
*   .fighters - KeyedContextVec
*   .stages   - KeyedContextVec
*   .meta     - PackageMeta
*   .rules    - Rules"#)
            }
            NodeToken::Custom (action, _) => {
                match action.as_ref() {
                    "save" => {
                        self.save()
                    }
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
                }
            }
            action => { format!("Package cannot '{:?}'", action) }
        };

        self.force_update_entire_package();
        result
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub enum Verify {
    Ok,
    None,
    IncorrectHash,
    UpdateAvailable,
    CannotConnect,
}

// Finer grained changes are used when speed is needed
#[derive(Clone, Serialize, Deserialize)]
pub enum PackageUpdate {
    Package (Package),
    DeleteFighterFrame { fighter: String, action: usize, frame_index: usize },
    InsertFighterFrame { fighter: String, action: usize, frame_index: usize, frame: ActionFrame },
    DeleteStage { index: usize, key: String },
    InsertStage { index: usize, key: String, stage: Stage },
}

/// Stores metadata for the package
/// Also handles updating the Package
#[derive(Clone, Default, Serialize, Deserialize, Node)]
pub struct PackageMeta {
    /// compared with a value incremented by canon collision when there are breaking changes to data structures
    pub engine_version:    u64,
    /// incremented every time the package is published
    pub published_version: u64,
    pub published:         bool,
    pub hash:              String,
    pub fighter_keys:      Vec<String>,
    pub stage_keys:        Vec<String>,
}

impl PackageMeta {
    pub fn new() -> PackageMeta {
        PackageMeta {
            engine_version:    engine_version(),
            published_version: 0,
            published:         false,
            hash:              "".to_string(),
            fighter_keys:      vec!(),
            stage_keys:        vec!(),
        }
    }
}
