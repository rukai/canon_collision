use treeflection::{Node, NodeRunner, NodeToken};

#[derive(Clone, Default, Serialize, Deserialize, Node)]
pub struct Rules {
    pub title:         String,
    pub goal:          Goal,
    pub stock_count:   u64,
    pub time_limit:    u64,
    pub best_of:       u64,
    pub teams:         bool,
    pub pause:         bool,
    pub friendly_fire: bool,
    pub ledge_grab:     LedgeGrab,
    pub grab_clang:     bool,
    //pub force_user_settings: User,
}

impl Rules {
    pub fn base() -> Rules {
        Rules {
            title:         "Base Game Mode".to_string(),
            goal:          Goal::Training,
            stock_count:   4,
            time_limit:    480,
            best_of:       3,
            pause:         true,
            teams:         false,
            friendly_fire: false,
            ledge_grab:    LedgeGrab::Hog,
            grab_clang:    false,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Node)]
pub enum Goal {
    Training,
    Time,
    Stock,
}

#[derive(Clone, Serialize, Deserialize, Node)]
pub enum LedgeGrab {
    Hog,
    Share,
    Steal
}

impl Default for Goal {
    fn default() -> Goal {
        Goal::Stock
    }
}

impl Default for LedgeGrab {
    fn default() -> LedgeGrab {
        LedgeGrab::Hog
    }
}
