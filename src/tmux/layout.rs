use crate::config;

pub use parser::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Layout {
    Pane(PaneGeom),
    H(PaneGeom, Vec<Layout>),
    V(PaneGeom, Vec<Layout>),
}

impl Layout {
    pub fn parse(input: &str) -> Result<Layout, Error> {
        Ok(parser::parse_layout(input)?.1)
    }

    pub fn geom(&self) -> &PaneGeom {
        match self {
            Layout::Pane(geom) => geom,
            Layout::H(geom, _) => geom,
            Layout::V(geom, _) => geom,
        }
    }

    pub fn width(&self) -> u32 {
        self.geom().width()
    }

    pub fn height(&self) -> u32 {
        self.geom().height()
    }
}

impl From<Layout> for config::Split {
    fn from(split: Layout) -> Self {
        const LINE_WIDTH: f32 = 1.0;

        match split {
            Layout::Pane(_) => config::Split::default(),
            Layout::H(_, mut splits) => {
                let last_split = match splits.pop() {
                    None => return config::Split::default(),
                    Some(split) => split,
                };
                let mut acc_width = last_split.width() as f32;
                let mut acc_split = last_split.into();

                // Build right-associative HSplit by traversing
                // the splits vector from right-to-left.
                for left_split in splits.into_iter().rev() {
                    let new_width = acc_width + left_split.width() as f32 - LINE_WIDTH;
                    let right_width_percent = (acc_width * 100f32 / new_width).round();
                    acc_split = config::Split::H {
                        left: config::HSplitPart {
                            width: None,
                            split: Box::new(left_split.into()),
                        },
                        right: config::HSplitPart {
                            width: Some(format!("{:.0}%", right_width_percent)),
                            split: Box::new(acc_split),
                        },
                    };
                    acc_width = new_width;
                }
                acc_split
            }
            Layout::V(_, mut splits) => {
                let last_split = match splits.pop() {
                    None => return config::Split::default(),
                    Some(split) => split,
                };
                let mut acc_height = last_split.height() as f32;
                let mut acc_split = last_split.into();

                // Build right-associative VSplit by traversing
                // the splits vector from right-to-left.
                for top_split in splits.into_iter().rev() {
                    let new_height = acc_height + top_split.height() as f32 - LINE_WIDTH;
                    let bottom_height_percent = (acc_height * 100f32 / new_height).round();
                    acc_split = config::Split::V {
                        top: config::VSplitPart {
                            height: None,
                            split: Box::new(top_split.into()),
                        },
                        bottom: config::VSplitPart {
                            height: Some(format!("{:.0}%", bottom_height_percent)),
                            split: Box::new(acc_split),
                        },
                    };
                    acc_height = new_height;
                }
                acc_split
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PaneGeom {
    pub size: Size,
    pub x_offset: u32,
    pub y_offset: u32,
}

impl PaneGeom {
    pub fn width(&self) -> u32 {
        self.size.width
    }

    pub fn height(&self) -> u32 {
        self.size.height
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

mod parser {
    use nom::{
        branch::alt,
        bytes::complete::{tag, take, take_until},
        character::complete::{digit1, u32},
        combinator::{all_consuming, map, value},
        multi::separated_list1,
        sequence::{delimited, pair, preceded, terminated, tuple},
        IResult,
    };
    use thiserror::Error;

    use super::*;

    pub(super) fn parse_layout(i: I) -> Result<Layout> {
        all_consuming(layout)(i)
    }

    #[derive(Debug, Error)]
    pub enum Error {
        #[error("layout parse error: {0}")]
        ParseError(String),
    }

    impl<E: std::error::Error> From<nom::Err<E>> for Error {
        fn from(err: nom::Err<E>) -> Self {
            Error::ParseError(format!("{}", err))
        }
    }

    type I<'a> = &'a str;
    type Result<'a, A> = IResult<I<'a>, A>;

    fn layout(i: I) -> Result<Layout> {
        preceded(checksum, split)(i)
    }

    fn split(i: I) -> Result<Layout> {
        alt((pane_split, h_split, v_split))(i)
    }

    fn pane_split(i: I) -> Result<Layout> {
        map(terminated(pane_geom, pair(tag(","), digit1)), Layout::Pane)(i)
    }

    fn h_split(i: I) -> Result<Layout> {
        map(
            pair(
                pane_geom,
                delimited(tag("{"), separated_list1(tag(","), split), tag("}")),
            ),
            |(pane, splits)| Layout::H(pane, splits),
        )(i)
    }

    fn v_split(i: I) -> Result<Layout> {
        map(
            pair(
                pane_geom,
                delimited(tag("["), separated_list1(tag(","), split), tag("]")),
            ),
            |(pane, splits)| Layout::V(pane, splits),
        )(i)
    }

    fn pane_geom(i: I) -> Result<PaneGeom> {
        map(
            tuple((size, tag(","), u32, tag(","), u32)),
            |(size, _, x_offset, _, y_offset)| PaneGeom {
                size,
                x_offset,
                y_offset,
            },
        )(i)
    }

    fn checksum(i: I) -> Result<()> {
        value((), tuple((take_until(","), take(1usize))))(i)
    }

    fn size(i: I) -> Result<Size> {
        map(tuple((u32, tag("x"), u32)), |(width, _, height)| {
            Size::new(width, height)
        })(i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample1() {
        let sample1 = "4264,401x112,0,0{200x112,0,0[200x56,0,0,546,200x55,0,57,798],200x112,201,0[200x56,201,0,795,200x55,201,57{100x55,201,57,796,99x55,302,57[99x27,302,57,797,99x27,302,85,799]}]}";
        let layout = Layout::parse(sample1).unwrap();

        use Layout::*;

        assert_eq!(
            layout,
            H(
                PaneGeom {
                    size: Size {
                        width: 401,
                        height: 112,
                    },
                    x_offset: 0,
                    y_offset: 0,
                },
                vec![
                    V(
                        PaneGeom {
                            size: Size {
                                width: 200,
                                height: 112,
                            },
                            x_offset: 0,
                            y_offset: 0,
                        },
                        vec![
                            Pane(PaneGeom {
                                size: Size {
                                    width: 200,
                                    height: 56,
                                },
                                x_offset: 0,
                                y_offset: 0,
                            },),
                            Pane(PaneGeom {
                                size: Size {
                                    width: 200,
                                    height: 55,
                                },
                                x_offset: 0,
                                y_offset: 57,
                            },),
                        ],
                    ),
                    V(
                        PaneGeom {
                            size: Size {
                                width: 200,
                                height: 112,
                            },
                            x_offset: 201,
                            y_offset: 0,
                        },
                        vec![
                            Pane(PaneGeom {
                                size: Size {
                                    width: 200,
                                    height: 56,
                                },
                                x_offset: 201,
                                y_offset: 0,
                            },),
                            H(
                                PaneGeom {
                                    size: Size {
                                        width: 200,
                                        height: 55,
                                    },
                                    x_offset: 201,
                                    y_offset: 57,
                                },
                                vec![
                                    Pane(PaneGeom {
                                        size: Size {
                                            width: 100,
                                            height: 55,
                                        },
                                        x_offset: 201,
                                        y_offset: 57,
                                    },),
                                    V(
                                        PaneGeom {
                                            size: Size {
                                                width: 99,
                                                height: 55,
                                            },
                                            x_offset: 302,
                                            y_offset: 57,
                                        },
                                        vec![
                                            Pane(PaneGeom {
                                                size: Size {
                                                    width: 99,
                                                    height: 27,
                                                },
                                                x_offset: 302,
                                                y_offset: 57,
                                            },),
                                            Pane(PaneGeom {
                                                size: Size {
                                                    width: 99,
                                                    height: 27,
                                                },
                                                x_offset: 302,
                                                y_offset: 85,
                                            },),
                                        ],
                                    ),
                                ],
                            ),
                        ],
                    ),
                ],
            )
        );
    }
}
