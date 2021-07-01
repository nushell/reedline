use crate::enums::EditCommand;

pub struct ViEngine {
    partial: Option<char>,
}

impl Default for ViEngine {
    fn default() -> ViEngine {
        ViEngine::new()
    }
}

impl ViEngine {
    pub fn new() -> Self {
        Self { partial: None }
    }

    pub fn handle(&mut self, commands: &[EditCommand]) -> Vec<EditCommand> {
        let mut output = vec![];
        for command in commands {
            match command {
                EditCommand::ViCommandFragment(c) => match (self.partial, c) {
                    (None, c) => match c {
                        'd' => self.partial = Some('d'),
                        'p' => {
                            output.push(EditCommand::PasteCutBuffer);
                        }
                        'h' => {
                            output.push(EditCommand::MoveLeft);
                        }
                        'l' => {
                            output.push(EditCommand::MoveRight);
                        }
                        'j' => {
                            output.push(EditCommand::PreviousHistory);
                        }
                        'k' => {
                            output.push(EditCommand::NextHistory);
                        }
                        'i' => {
                            output.push(EditCommand::EnterViInsert);
                        }
                        _ => {}
                    },
                    (Some(partial), c) => {
                        if partial == 'd' {
                            match c {
                                'd' => {
                                    output.push(EditCommand::MoveToStart);
                                    output.push(EditCommand::CutToEnd);
                                }
                                'w' => {
                                    output.push(EditCommand::CutWordRight);
                                }
                                _ => {}
                            }
                        }
                        self.partial = None;
                    }
                },
                x => {
                    output.push(x.clone());
                }
            }
        }
        output
    }
}
