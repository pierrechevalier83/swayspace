#![feature(iter_partition_in_place)]

use std::str::FromStr;
use structopt::StructOpt;
use swayipc::Connection;

#[derive(Debug)]
enum To {
    Prev,
    Next,
}

impl FromStr for To {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "prev-workspace" => Ok(Self::Prev),
            "next-workspace" => Ok(Self::Next),
            s => Err(format!("Failed to parse {} as --to. Expected one of [prev-workspace, next-workspace]", s))
        }
    }
}

impl std::fmt::Display for To {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            To::Next => write!(f, "next"),
            To::Prev => write!(f, "prev"),
        }
    }
}

struct WindowManagerState {
    current_workspace: i32,
    workspaces_on_focused_output: Vec<i32>,
    workspaces_on_unfocused_outputs: Vec<i32>,
    max_workspace_on_focused_output: i32,
    min_workspace_on_focused_output: i32,
    is_current_workspace_empty: bool,
}

impl WindowManagerState {
    fn from_wm(wm: &mut Connection) -> Self {
        let focused_output_name = wm
            .get_tree()
            .unwrap()
            .find_focused(|node| match node.node_type {
                swayipc::reply::NodeType::Output => true,
                _ => false,
            })
            .unwrap()
            .name
            .unwrap();

        let mut all_workspaces = wm.get_workspaces().unwrap();
        let current_workspace = all_workspaces.iter().find(|w| w.focused).unwrap();
        let is_current_workspace_empty = current_workspace.representation == "";
        let current_workspace = current_workspace.num;
        let partition_point = all_workspaces
            .iter_mut()
            .partition_in_place(|w| w.output == focused_output_name);
        let workspaces_on_focused_output = all_workspaces[0..partition_point]
            .iter()
            .map(|w| w.num)
            .collect::<Vec<_>>();
        let workspaces_on_unfocused_outputs = all_workspaces[partition_point..]
            .iter()
            .map(|w| w.num)
            .collect::<Vec<_>>();
        let max_workspace_on_focused_output = *workspaces_on_focused_output.iter().max().unwrap();
        let min_workspace_on_focused_output = *workspaces_on_focused_output.iter().min().unwrap();
        Self {
            current_workspace,
            workspaces_on_focused_output,
            workspaces_on_unfocused_outputs,
            max_workspace_on_focused_output,
            min_workspace_on_focused_output,
            is_current_workspace_empty,
        }
    }
    fn make_new_workspace_at_end(&self) -> i32 {
        let mut index = self.max_workspace_on_focused_output + 1;
        // skip over any existing workspaces on unfocused outputs and pick the next_available number
        while self.workspaces_on_unfocused_outputs.contains(&index) {
            index += 1;
        }
        index
    }
    fn make_new_workspace_at_start(&self) -> i32 {
        let index = self.min_workspace_on_focused_output;

        (1..index)
            .rev()
            .find(|num| !self.workspaces_on_unfocused_outputs.contains(&num))
            .unwrap_or(index)
    }
    fn next_workspace_on_focused_output(&self) -> i32 {
        if self.current_workspace == self.max_workspace_on_focused_output
            && self.is_current_workspace_empty
            && self.workspaces_on_focused_output.len() > 1
        {
            self.min_workspace_on_focused_output
        } else {
            self.workspaces_on_focused_output
                .iter()
                .filter(|&num| num > &self.current_workspace)
                .min()
                .copied()
                .unwrap_or(self.make_new_workspace_at_end())
        }
    }
    fn prev_workspace_on_focused_output(&self) -> i32 {
        if self.current_workspace == self.min_workspace_on_focused_output
            && self.is_current_workspace_empty
        {
            self.make_new_workspace_at_start()
        } else {
            self.workspaces_on_focused_output
                .iter()
                .filter(|&num| num < &self.current_workspace)
                .max()
                .copied()
                .unwrap_or(self.make_new_workspace_at_end())
        }
    }
}

fn pick_destination(wm: &mut Connection, opt: Opt) -> i32 {
    let wm_state = WindowManagerState::from_wm(wm);
    match opt.to {
        To::Next => wm_state.next_workspace_on_focused_output(),
        To::Prev => wm_state.prev_workspace_on_focused_output(),
    }
}

#[derive(Debug)]
enum Do {
    MoveFocus,
    MoveContainer,
}

impl FromStr for Do {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "move-focus" => Ok(Self::MoveFocus),
            "move-container" => Ok(Self::MoveContainer),
            _ => Err(format!("Failed to parse {} as --do. Expected one of [move-focus, move-container]", s)),
        }
    }
}

#[derive(Debug, StructOpt)]
#[structopt(about = "Automatically create workspaces under sway like gnome does")]
struct Opt {
    #[structopt(long="do", default_value = "move-focus")]
    command: Do,
    #[structopt(long="to", default_value = "next-workspace")]
    to: To,
}

fn main() {
    pretty_env_logger::init();
    let opt = Opt::from_args();
    let mut wm = swayipc::Connection::new().unwrap();
    match opt.command {
        Do::MoveFocus => {
            let destination = pick_destination(&mut wm, opt);
            wm.run_command(format!("workspace number {}", destination))
                .unwrap();
        }
        Do::MoveContainer => {
            let destination = pick_destination(&mut wm, opt);
            wm.run_command(format!("move container to workspace number {}", destination))
                .unwrap();
            wm.run_command(format!("workspace number {}", destination))
                .unwrap();
        }
    }
}
