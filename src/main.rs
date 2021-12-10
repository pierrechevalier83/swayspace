#![feature(iter_partition_in_place)]

use std::str::FromStr;
use structopt::StructOpt;
use swayipc::Connection;

#[derive(Debug)]
enum Workspace {
    Prev,
    Next,
    Num(i32),
}

impl FromStr for Workspace {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "prev" => Ok(Self::Prev),
            "next" => Ok(Self::Next),
            s => Ok(Self::Num(
                s.parse::<i32>()
                    .map_err(|_| format!("Can't parse {} as Command", s))?,
            )),
        }
    }
}

impl std::fmt::Display for Workspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Workspace::Next => write!(f, "next"),
            Workspace::Prev => write!(f, "prev"),
            Workspace::Num(n) => write!(f, "number {}", n),
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

fn pick_destination(wm: &mut Connection, opt: Opt) -> Workspace {
    let wm_state = WindowManagerState::from_wm(wm);
    Workspace::Num(match opt.to {
        Workspace::Next => wm_state.next_workspace_on_focused_output(),
        Workspace::Prev => wm_state.prev_workspace_on_focused_output(),
        Workspace::Num(n) => n,
    })
}

#[derive(Debug)]
enum Command {
    MoveToWorkspace,
    MoveContainerToWorkspace,
}

impl FromStr for Command {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "workspace" => Ok(Self::MoveToWorkspace),
            "move-container-to-workspace" => Ok(Self::MoveContainerToWorkspace),
            _ => Err(format!("Can't parse {} as Command", s)),
        }
    }
}

#[derive(Debug, StructOpt)]
#[structopt(about = "Automatically create workspaces under sway like gnome does")]
struct Opt {
    #[structopt(default_value = "workspace")]
    command: Command,
    to: Workspace,
}

fn main() {
    pretty_env_logger::init();
    let opt = Opt::from_args();
    let mut wm = swayipc::Connection::new().unwrap();
    match opt.command {
        Command::MoveToWorkspace => {
            let destination = pick_destination(&mut wm, opt);
            wm.run_command(format!("workspace {}", destination))
                .unwrap();
        }
        Command::MoveContainerToWorkspace => {
            let destination = pick_destination(&mut wm, opt);
            wm.run_command(format!("move container to workspace {}", destination))
                .unwrap();
            wm.run_command(format!("workspace {}", destination))
                .unwrap();
        }
    }
}
