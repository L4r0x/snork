use std::cmp::Reverse;
use std::collections::VecDeque;
use std::ops::{Index, IndexMut};

use super::{Cell, Grid, Snake};
use crate::env::{Direction, Vec2D};

/// Floodfill Cell that stores the important data in a single Byte.
///
/// Bitfield:
/// - 7: you?
/// - 6..0: num (free if n = 0, owned if n = 2^7 - 1, otherwise occupied)
#[derive(Clone, Copy)]
pub struct FCell(u8);

const FCELL_YOU: u8 = 0b1000_0000;
const FCELL_NUM: u8 = 0b0111_1111;
const FCELL_OWNED: u8 = 0b0111_1111;

impl FCell {
    #[inline]
    pub const fn free() -> FCell {
        FCell(0)
    }

    #[inline]
    pub const fn with_owner(you: bool) -> FCell {
        FCell((you as u8) << 7 | FCELL_OWNED)
    }

    #[inline]
    pub const fn with_occupier(you: bool, num: u8) -> FCell {
        FCell((you as u8) << 7 | num & FCELL_NUM)
    }

    #[inline]
    pub const fn is_occupied(&self) -> bool {
        let num = self.0 & FCELL_NUM;
        num != 0 && num != FCELL_OWNED
    }

    #[inline]
    pub const fn is_you(&self) -> bool {
        self.0 & FCELL_YOU != 0
    }

    #[inline]
    pub const fn get_num(&self) -> u8 {
        self.0 & FCELL_NUM
    }

    #[inline]
    pub const fn is_free(&self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub const fn is_owned(&self) -> bool {
        self.0 & FCELL_NUM == FCELL_OWNED
    }
}

impl std::fmt::Debug for FCell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_free() {
            write!(f, "___")
        } else {
            if self.is_you() {
                write!(f, "+")?;
            } else {
                write!(f, "#")?;
            }
            if self.is_occupied() {
                write!(f, "{:0>2}", self.get_num())
            } else {
                write!(f, "..")
            }
        }
    }
}

/// Grid that performs the floodfill algorithm asses area control.
///
/// This struct also contains all necessary buffers for the floodfill algorithm.
pub struct FloodFill {
    cells: Vec<FCell>,
    queue: VecDeque<(bool, u8, u8, Vec2D)>,
    pub width: usize,
    pub height: usize,
}

impl FloodFill {
    pub fn new(width: usize, height: usize) -> FloodFill {
        FloodFill {
            cells: vec![FCell::free(); width * height],
            queue: VecDeque::with_capacity(width * height),
            width,
            height,
        }
    }

    /// Returns if `p` is within the boundaries of the board.
    pub fn has(&self, p: Vec2D) -> bool {
        0 <= p.x && p.x < self.width as _ && 0 <= p.y && p.y < self.height as _
    }

    /// Counts the space of you or the enemies.
    pub fn count_space_of(&self, you: bool) -> usize {
        self.cells
            .iter()
            .filter(|&c| c.is_owned() && c.is_you() == you)
            .count()
    }

    /// Clears the board so that it can be reused for another floodfill computation.
    pub fn clear(&mut self) {
        for c in &mut self.cells {
            *c = FCell::free();
        }
        self.queue.clear();
    }

    /// Flood fill combined with ignoring tails depending on distance to head.
    ///
    /// The board has to be cleared and prepared in beforehand.
    ///
    /// The general idea is to track the distance n from the heads and ignoring
    /// snake bodies that are vanishing after n moves.
    /// This allows the snake to follow its tail or enemy tails.
    ///
    /// Food on the way is been accounted for the own tail.
    pub fn flood(&mut self, grid: &Grid, heads: impl Iterator<Item = (bool, Vec2D)>) {
        assert_eq!(self.width, grid.width);
        assert_eq!(self.height, grid.height);

        #[inline]
        fn owns(cell: FCell, you: bool, num: u8, food: u8) -> bool {
            cell.is_free()
                || (cell.is_occupied()
                    // follow your tail
                    && ((cell.is_you() == you && cell.get_num() <= num - food)
                        // follow enemy tail
                        // distance of 1 as buffer for eating
                        || (cell.is_you() != you && cell.get_num() <= num - you as u8)))
        }

        for (you, p) in heads {
            if self.has(p) {
                let num = 1;
                let food = if grid[p] == Cell::Food { 1 } else { 0 };
                let cell = self[p];
                if owns(cell, you, num, food) {
                    // println!(">> ({}, {}, {}, {:?}), {:?}", you, num, food, p, cell);
                    self[p] = FCell::with_owner(you);
                    self.queue.push_back((you, num + 1, food, p));
                }
            }
        }

        while let Some((you, num, mut food, p)) = self.queue.pop_front() {
            for dir in Direction::iter() {
                let p = p.apply(dir);
                if self.has(p) {
                    if grid[p] == Cell::Food {
                        food += 1;
                    }
                    let cell = self[p];
                    // println!("({}, {}, {}, {:?}), {:?}", you, num, food, p, cell);
                    if owns(cell, you, num, food) {
                        self[p] = FCell::with_owner(you);
                        self.queue.push_back((you, num + 1, food, p));
                    }
                }
            }
        }
    }

    /// Prepare the board and compute flood fill.
    /// It is assumed that the snake at position 0 is the evaluated agent and
    /// the other snakes are the enemies.
    pub fn flood_snakes(&mut self, grid: &Grid, snakes: &[Snake], you_i: u8) {
        self.clear();
        let mut snakes: Vec<&Snake> = snakes.iter().filter(|s| s.alive()).collect();

        // Prepare board with snakes (tail = 1, ..., head = n)
        for snake in &snakes {
            for (i, p) in snake.body.iter().enumerate() {
                let num = if (i as u8) < FCELL_OWNED {
                    i as u8 + 1
                } else {
                    FCELL_OWNED
                };
                self[*p] = FCell::with_occupier(snake.id == you_i, num)
            }
        }

        // Longer or equally long snakes first
        snakes.sort_by_key(|s| Reverse(2 * s.body.len() - (s.id == you_i) as usize));
        self.flood(
            grid,
            snakes
                .iter()
                .flat_map(|s| Direction::iter().map(move |d| (s.id == you_i, s.head().apply(d)))),
        );
    }
}

impl Index<Vec2D> for FloodFill {
    type Output = FCell;

    fn index(&self, p: Vec2D) -> &Self::Output {
        assert!(0 <= p.x && p.x < self.width as _);
        assert!(0 <= p.y && p.y < self.height as _);
        &self.cells[(p.x as usize % self.width + p.y as usize * self.width) as usize]
    }
}

impl IndexMut<Vec2D> for FloodFill {
    fn index_mut(&mut self, p: Vec2D) -> &mut Self::Output {
        assert!(0 <= p.x && p.x < self.width as _);
        assert!(0 <= p.y && p.y < self.height as _);
        &mut self.cells[(p.x as usize % self.width + p.y as usize * self.width) as usize]
    }
}

impl std::fmt::Debug for FloodFill {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Grid {{")?;
        for y in 0..self.height as i16 {
            write!(f, "  ")?;
            for x in 0..self.width as i16 {
                write!(f, "{:?} ", self[Vec2D::new(x, self.height as i16 - y - 1)])?;
            }
            writeln!(f)?;
        }
        write!(f, "}}")?;
        Ok(())
    }
}

#[cfg(test)]
mod test {

    #[test]
    fn flood_head() {
        use super::*;
        let grid = Grid::new(11, 11);

        let mut floodfill = FloodFill::new(grid.width, grid.height);
        floodfill.flood(&grid, [(true, Vec2D::new(0, 0))].iter().cloned());
        println!("Filled {} {:?}", floodfill.count_space_of(true), floodfill);
        assert_eq!(floodfill.count_space_of(true), 11 * 11);

        let grid = Grid::new(11, 11);
        floodfill.clear();
        floodfill.flood(
            &grid,
            [(true, Vec2D::new(0, 0)), (false, Vec2D::new(10, 10))]
                .iter()
                .cloned(),
        );
        println!("Filled {} {:?}", floodfill.count_space_of(true), floodfill);
        assert_eq!(floodfill.count_space_of(true), 66);
        let grid = Grid::new(11, 11);
        floodfill.clear();
        floodfill.flood(
            &grid,
            [
                (false, Vec2D::new(0, 0)),
                (false, Vec2D::new(10, 10)),
                (true, Vec2D::new(5, 5)),
            ]
            .iter()
            .cloned(),
        );
        println!("Filled {} {:?}", floodfill.count_space_of(true), floodfill);
        assert_eq!(floodfill.count_space_of(true), 61);
    }

    #[test]
    fn flood_snakes_follow_tail() {
        use super::*;
        let grid = Grid::new(11, 11);
        let mut floodfill = FloodFill::new(grid.width, grid.height);
        let snakes = [Snake::new(
            0,
            vec![
                Vec2D::new(0, 0),
                Vec2D::new(0, 1),
                Vec2D::new(0, 2),
                Vec2D::new(1, 2),
                Vec2D::new(2, 2),
                Vec2D::new(3, 2),
                Vec2D::new(3, 1),
                Vec2D::new(3, 0),
                Vec2D::new(2, 0),
                Vec2D::new(1, 0),
            ]
            .into(),
            100,
        )];

        floodfill.flood_snakes(&grid, &snakes, 0);
        println!("Filled {} {:?}", floodfill.count_space_of(true), floodfill);
        assert_eq!(floodfill.count_space_of(true), 11 * 11);
    }

    #[test]
    fn flood_snakes_bite_tail() {
        use super::*;
        let grid = Grid::new(11, 11);
        let mut floodfill = FloodFill::new(grid.width, grid.height);
        let snakes = [Snake::new(
            0,
            vec![
                Vec2D::new(0, 0),
                Vec2D::new(0, 0),
                Vec2D::new(0, 1),
                Vec2D::new(0, 2),
                Vec2D::new(1, 2),
                Vec2D::new(2, 2),
                Vec2D::new(3, 2),
                Vec2D::new(3, 1),
                Vec2D::new(3, 0),
                Vec2D::new(2, 0),
                Vec2D::new(1, 0),
            ]
            .into(),
            100,
        )];

        floodfill.flood_snakes(&grid, &snakes, 0);
        println!("Filled {} {:?}", floodfill.count_space_of(true), floodfill);
        assert_eq!(floodfill.count_space_of(true), 2);
    }

    #[test]
    fn flood_snakes_bite_food() {
        use super::*;
        let mut grid = Grid::new(11, 11);
        grid[Vec2D::new(1, 0)] = Cell::Food;
        println!("{:?}", grid);
        let mut floodfill = FloodFill::new(grid.width, grid.height);
        let snakes = [Snake::new(
            0,
            vec![
                Vec2D::new(0, 0),
                Vec2D::new(0, 0),
                Vec2D::new(0, 1),
                Vec2D::new(1, 1),
                Vec2D::new(2, 1),
                Vec2D::new(3, 1),
                Vec2D::new(3, 0),
                Vec2D::new(2, 0),
            ]
            .into(),
            100,
        )];

        floodfill.flood_snakes(&grid, &snakes, 0);
        println!("Filled {} {:?}", floodfill.count_space_of(true), floodfill);
        assert_eq!(floodfill.count_space_of(true), 1);

        let snakes = [Snake::new(
            0,
            vec![
                Vec2D::new(0, 0),
                Vec2D::new(0, 1),
                Vec2D::new(1, 1),
                Vec2D::new(2, 1),
                Vec2D::new(3, 1),
                Vec2D::new(3, 0),
                Vec2D::new(2, 0),
            ]
            .into(),
            100,
        )];

        floodfill.clear();
        floodfill.flood_snakes(&grid, &snakes, 0);
        println!("Filled {} {:?}", floodfill.count_space_of(true), floodfill);
        assert_eq!(floodfill.count_space_of(true), 11 * 11);
    }

    #[test]
    fn flood_snakes_enemy() {
        use super::*;
        let mut grid = Grid::new(11, 11);
        grid[Vec2D::new(1, 0)] = Cell::Food;
        println!("{:?}", grid);
        let mut floodfill = FloodFill::new(grid.width, grid.height);
        let snakes = [
            Snake::new(
                0,
                vec![
                    Vec2D::new(1, 2),
                    Vec2D::new(1, 1),
                    Vec2D::new(2, 1),
                    Vec2D::new(2, 0),
                    Vec2D::new(1, 0),
                ]
                .into(),
                100,
            ),
            Snake::new(
                1,
                vec![Vec2D::new(0, 1), Vec2D::new(0, 2), Vec2D::new(0, 3)].into(),
                100,
            ),
        ];
        floodfill.flood_snakes(&grid, &snakes, 0);
        println!("Filled {} {:?}", floodfill.count_space_of(true), floodfill);
        assert_eq!(floodfill.count_space_of(true), 24);
    }
}
