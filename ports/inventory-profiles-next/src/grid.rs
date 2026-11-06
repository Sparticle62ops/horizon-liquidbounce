//! The container model: a width-by-height grid of item stacks, the shape IPN
//! operates on. Stacks are (name, count); stacks only stack when names match
//! (no per-stack max sizes yet, matching how the sort cares about identity).

#[derive(Clone, Debug, PartialEq)]
pub struct Stack {
    pub name: String,
    pub count: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Grid {
    pub width: usize,
    pub stacks: Vec<Option<Stack>>,
}

impl Grid {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, stacks: vec![None; width * height] }
    }

    pub fn len(&self) -> usize { self.stacks.len() }

    pub fn get(&self, slot: usize) -> Option<&Option<Stack>> {
        self.stacks.get(slot)
    }

    pub fn set(&mut self, slot: usize, s: Option<Stack>) {
        if slot < self.stacks.len() {
            self.stacks[slot] = s;
        }
    }

    /// Slots that hold a stack, ascending.
    pub fn filled(&self) -> Vec<usize> {
        (0..self.stacks.len()).filter(|i| self.stacks[*i].is_some()).collect()
    }

    /// Total stack count of one item name across the grid.
    pub fn total_of(&self, name: &str) -> u32 {
        self.stacks.iter().flatten().filter(|s| s.name == name).map(|s| s.count).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(name: &str, count: u32) -> Option<Stack> {
        Some(Stack { name: name.into(), count })
    }

    #[test]
    fn grid_shape_and_access() {
        let mut g = Grid::new(3, 2);
        assert_eq!(g.len(), 6);
        g.set(1, st("minecraft:diamond", 3));
        assert_eq!(g.get(1).unwrap().as_ref().unwrap().name, "minecraft:diamond");
        assert!(g.get(0).unwrap().is_none());
        assert!(g.get(6).is_none(), "out of bounds is None, not a panic");
    }

    #[test]
    fn filled_reports_only_occupied_slots() {
        let mut g = Grid::new(3, 2);
        g.set(0, st("a", 1));
        g.set(4, st("b", 1));
        assert_eq!(g.filled(), vec![0, 4]);
        assert_eq!(g.total_of("a"), 1);
    }
}
