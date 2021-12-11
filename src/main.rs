#![feature(iter_partition_in_place)]

use clap::arg_enum;
use std::str::FromStr;
use structopt::StructOpt;
use swayipc::Connection;

arg_enum! {
    #[derive(Debug, Clone, Copy)]
enum To {
    Workspace,
    Output,
}
}

arg_enum! {
    #[derive(Debug, Clone, Copy)]
enum Direction {
    Prev,
    Next,
}
}

#[derive(Debug)]
enum Do {
    MoveFocusTo,
    MoveContainerTo,
}

impl FromStr for Do {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "move-focus-to" => Ok(Self::MoveFocusTo),
            "move-container-to" => Ok(Self::MoveContainerTo),
            _ => Err(format!(
                "Failed to parse {} as --do. Expected one of [move-focus-to, move-container-to]",
                s
            )),
        }
    }
}

#[derive(Debug, StructOpt)]
#[structopt(about = "Automatically create workspaces under sway like gnome does")]
struct Opt {
    #[structopt(default_value = "move-focus-to")]
    command: Do,
    #[structopt(default_value = "workspace", possible_values = &To::variants(), case_insensitive = true)]
    to: To,
    #[structopt(default_value = "next", possible_values = &Direction::variants(), case_insensitive = true)]
    dir: Direction,
    #[structopt(long = "walk-into-gaps")]
    walk_into_gaps: bool,
    #[structopt(long = "static")]
    static_behaviour: bool,
}

struct WindowManagerState {
    current_workspace: i32,
    workspaces_on_focused_output: Vec<i32>,
    workspaces_on_unfocused_outputs: Vec<i32>,
    max_workspace_on_focused_output: i32,
    is_max_workspace_empty: bool,
    // For each output in order of its x position, the num of its visible workspace
    visible_workspace_per_output: Vec<i32>,
}

#[derive(PartialEq, Eq, Ord, PartialOrd)]
struct Output {
    x_pos: i64,
    y_pos: i64,
    name: String,
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

        let mut outputs = wm
            .get_outputs()
            .unwrap()
            .iter()
            .map(|o| Output {
                x_pos: o.rect.x,
                y_pos: o.rect.y,
                name: o.name.clone(),
            })
            .collect::<Vec<_>>();

        outputs.sort();

        let mut all_workspaces = wm.get_workspaces().unwrap();
        let visible_workspaces = all_workspaces
            .iter()
            .filter(|w| w.visible)
            .collect::<Vec<_>>();
        let visible_workspace_per_output = outputs
            .iter()
            .filter_map(|o| {
                visible_workspaces
                    .iter()
                    .find(|w| w.output == o.name)
                    .map(|w| w.num)
            })
            .collect();

        let current_workspace = all_workspaces.iter().find(|w| w.focused).unwrap().num;
        let partition_point = all_workspaces
            .iter_mut()
            .partition_in_place(|w| w.output == focused_output_name);
        let mut workspaces_on_focused_output = all_workspaces[0..partition_point]
            .iter()
            .map(|w| w.num)
            .collect::<Vec<_>>();
        workspaces_on_focused_output.sort_unstable();
        let workspaces_on_unfocused_outputs = all_workspaces[partition_point..]
            .iter()
            .map(|w| w.num)
            .collect::<Vec<_>>();
        let max_workspace_on_focused_output = *workspaces_on_focused_output.iter().max().unwrap();
        let max_workspace = all_workspaces
            .iter()
            .find(|w| w.num == max_workspace_on_focused_output)
            .unwrap();
        let is_max_workspace_empty = max_workspace.representation == "";
        Self {
            current_workspace,
            workspaces_on_focused_output,
            workspaces_on_unfocused_outputs,
            max_workspace_on_focused_output,
            is_max_workspace_empty,
            visible_workspace_per_output,
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
    // Note: only to be called with a single workspace on this display
    fn make_new_workspace_at_start(&self) -> i32 {
        (1..self.current_workspace)
            .rev()
            .find(|num| !self.workspaces_on_unfocused_outputs.contains(&num))
            .unwrap_or(self.current_workspace)
    }
    fn cycle_through<'a, It>(
        &'a self,
        workspaces: It,
        dir: Direction,
        static_behaviour: bool,
    ) -> Option<i32>
    where
        It: Iterator<Item = i32>
            + DoubleEndedIterator<Item = i32>
            + Sized
            + ExactSizeIterator
            + Clone
            + 'a,
    {
        let iter = workspaces
            .chain({
                if static_behaviour
                    || (self.is_max_workspace_empty && self.workspaces_on_focused_output.len() > 1)
                {
                    None
                } else {
                    Some(self.make_new_workspace_at_end().try_into().unwrap())
                }
                .into_iter()
            })
            .chain(
                if static_behaviour
                    || !(self.is_max_workspace_empty
                        && self.workspaces_on_focused_output.len() == 1)
                {
                    None
                } else {
                    Some(self.make_new_workspace_at_start().try_into().unwrap())
                },
            );
        match dir {
            Direction::Next => iter
                .cycle()
                .skip_while(|&w| w != self.current_workspace)
                .skip(1)
                .next(),
            Direction::Prev => iter
                .rev()
                .cycle()
                .skip_while(|&w| w != self.current_workspace)
                .skip(1)
                .next(),
        }
    }
    fn cycle_through_workspaces_on_focused_output(
        &self,
        walk_into_gaps: bool,
        dir: Direction,
        static_behaviour: bool,
    ) -> i32 {
        match walk_into_gaps {
            true => self
                .cycle_through(
                    (1..=self.max_workspace_on_focused_output)
                        .filter(|w| !self.workspaces_on_unfocused_outputs.contains(&w))
                        .collect::<Vec<_>>()
                        .into_iter(),
                    dir,
                    static_behaviour,
                )
                .unwrap(),
            false => self
                .cycle_through(
                    self.workspaces_on_focused_output.iter().copied(),
                    dir,
                    static_behaviour,
                )
                .unwrap(),
        }
    }
    fn cycle_through_outputs(&self, dir: Direction) -> i32 {
        self.cycle_through(self.visible_workspace_per_output.iter().copied(), dir, true)
            .unwrap()
    }
}

fn pick_destination(wm_state: &WindowManagerState, opt: &Opt) -> i32 {
    match (opt.to, opt.dir) {
        (To::Workspace, dir) => wm_state.cycle_through_workspaces_on_focused_output(
            opt.walk_into_gaps,
            dir,
            opt.static_behaviour,
        ),
        (To::Output, dir) => wm_state.cycle_through_outputs(dir),
    }
}

fn main() {
    pretty_env_logger::init();
    let opt = Opt::from_args();
    let mut wm = swayipc::Connection::new().unwrap();
    let wm_state = WindowManagerState::from_wm(&mut wm);
    match opt.command {
        Do::MoveFocusTo => {
            let destination = pick_destination(&wm_state, &opt);
            wm.run_command(format!("workspace number {}", destination))
                .unwrap();
        }
        Do::MoveContainerTo => {
            let destination = pick_destination(&wm_state, &opt);
            wm.run_command(format!(
                "move container to workspace number {}",
                destination
            ))
            .unwrap();
            wm.run_command(format!("workspace number {}", destination))
                .unwrap();
        }
    }
}
