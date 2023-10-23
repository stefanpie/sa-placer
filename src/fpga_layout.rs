use rustc_hash::FxHashMap;

#[derive(Clone, Hash, PartialEq, Eq, Debug, Copy)]
pub enum MacroType {
    CLB,
    DSP,
    BRAM,
    IO,
}

#[derive(Clone, Hash, PartialEq, Eq, Debug)]
pub enum FPGALayoutType {
    MacroType(MacroType),
    EMPTY,
}

#[derive(Eq, Hash, PartialEq, Clone, Copy, Debug)]
pub struct FPGALayoutCoordinate {
    pub x: u32,
    pub y: u32,
}

impl FPGALayoutCoordinate {
    pub fn new(x: u32, y: u32) -> FPGALayoutCoordinate {
        FPGALayoutCoordinate { x, y }
    }
}

#[derive(Debug, Clone)]
pub struct FPGALayout {
    pub map: FxHashMap<FPGALayoutCoordinate, FPGALayoutType>,
    pub width: u32,
    pub height: u32,
}

impl FPGALayout {
    pub fn new(width: u32, height: u32) -> FPGALayout {
        FPGALayout {
            map: FxHashMap::default(),
            width,
            height,
        }
    }

    pub fn config_corners(&mut self, layout_type: FPGALayoutType) {
        self.map
            .insert(FPGALayoutCoordinate::new(0, 0), layout_type.clone());
        self.map.insert(
            FPGALayoutCoordinate::new(0, self.height - 1),
            layout_type.clone(),
        );
        self.map.insert(
            FPGALayoutCoordinate::new(self.width - 1, 0),
            layout_type.clone(),
        );
        self.map.insert(
            FPGALayoutCoordinate::new(self.width - 1, self.height - 1),
            layout_type.clone(),
        );
    }

    pub fn config_border(&mut self, layout_type: FPGALayoutType) {
        for x in 0..self.width {
            self.map
                .insert(FPGALayoutCoordinate::new(x, 0), layout_type.clone());
            self.map.insert(
                FPGALayoutCoordinate::new(x, self.height - 1),
                layout_type.clone(),
            );
        }

        for y in 0..self.height {
            self.map
                .insert(FPGALayoutCoordinate::new(0, y), layout_type.clone());
            self.map.insert(
                FPGALayoutCoordinate::new(self.width - 1, y),
                layout_type.clone(),
            );
        }
    }

    pub fn config_repeat(
        &mut self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        step_x: u32,
        step_y: u32,
        layout_type: FPGALayoutType,
    ) {
        for x in (x..(x + width)).step_by(step_x as usize) {
            for y in (y..(y + height)).step_by(step_y as usize) {
                if x >= self.width || y >= self.height {
                    continue;
                }
                self.map
                    .insert(FPGALayoutCoordinate::new(x, y), layout_type.clone());
            }
        }
    }

    pub fn valid(&mut self) -> bool {
        // make sure all entries are inside the width and height
        for coord in self.map.keys() {
            if coord.x >= self.width || coord.y >= self.height {
                return false;
            }
        }
        true
    }

    pub fn get(&self, coordinate: &FPGALayoutCoordinate) -> Option<FPGALayoutType> {
        if coordinate.x >= self.width || coordinate.y >= self.height {
            return None;
        }
        if !self.map.contains_key(coordinate) {
            return Some(FPGALayoutType::EMPTY);
        }
        self.map.get(coordinate).cloned()
    }

    pub fn count_summary(&self) -> FxHashMap<FPGALayoutType, u32> {
        let mut count_summary = FxHashMap::default();

        let mut empty_count = 0;
        let mut clb_count = 0;
        let mut dsp_count = 0;
        let mut bram_count = 0;
        let mut io_count = 0;

        for x in 0..self.width {
            for y in 0..self.height {
                let layout_type = self.get(&FPGALayoutCoordinate::new(x, y)).unwrap();

                match layout_type {
                    FPGALayoutType::MacroType(MacroType::CLB) => clb_count += 1,
                    FPGALayoutType::MacroType(MacroType::DSP) => dsp_count += 1,
                    FPGALayoutType::MacroType(MacroType::BRAM) => bram_count += 1,
                    FPGALayoutType::MacroType(MacroType::IO) => io_count += 1,
                    FPGALayoutType::EMPTY => empty_count += 1,
                }
            }
        }

        count_summary.insert(FPGALayoutType::EMPTY, empty_count);
        count_summary.insert(FPGALayoutType::MacroType(MacroType::CLB), clb_count);
        count_summary.insert(FPGALayoutType::MacroType(MacroType::DSP), dsp_count);
        count_summary.insert(FPGALayoutType::MacroType(MacroType::BRAM), bram_count);
        count_summary.insert(FPGALayoutType::MacroType(MacroType::IO), io_count);

        count_summary
    }

    pub fn render_summary(&mut self) -> String {
        let mut output: String = String::new();

        // print the x, y size of the descive
        // print the number of each type of macro type

        output.push_str("FPGA Layout Summary\n");
        output.push_str(&format!("Width: {}\n", self.width));
        output.push_str(&format!("Height: {}\n", self.height));

        let mut clb_count = 0;
        let mut dsp_count = 0;
        let mut bram_count = 0;
        let mut io_count = 0;
        let mut empty_count = 0;

        for x in 0..self.width {
            for y in 0..self.height {
                let layout_type = self
                    .map
                    .get(&FPGALayoutCoordinate::new(x, y))
                    .unwrap_or(&FPGALayoutType::EMPTY);

                match layout_type {
                    FPGALayoutType::MacroType(MacroType::CLB) => clb_count += 1,
                    FPGALayoutType::MacroType(MacroType::DSP) => dsp_count += 1,
                    FPGALayoutType::MacroType(MacroType::BRAM) => bram_count += 1,
                    FPGALayoutType::MacroType(MacroType::IO) => io_count += 1,
                    FPGALayoutType::EMPTY => empty_count += 1,
                }
            }
        }

        output.push_str(&format!("CLB Count: {}\n", clb_count));
        output.push_str(&format!("DSP Count: {}\n", dsp_count));
        output.push_str(&format!("BRAM Count: {}\n", bram_count));
        output.push_str(&format!("IO Count: {}\n", io_count));
        output.push_str(&format!("Empty Count: {}\n", empty_count));

        output
    }

    pub fn render_ascii(&self) -> String {
        let mut output = String::new();

        // Draw the top line
        for x in 0..self.width {
            if x == 0 {
                output.push_str("┌───");
            } else {
                output.push_str("┬───");
            }
        }
        output.push_str("┐\n");

        for y in 0..self.height {
            // Draw the cells
            for x in 0..self.width {
                let layout_type = self
                    .map
                    .get(&FPGALayoutCoordinate::new(x, y))
                    .unwrap_or(&FPGALayoutType::EMPTY);

                match layout_type {
                    FPGALayoutType::MacroType(MacroType::CLB) => output.push_str("│ C "),
                    FPGALayoutType::MacroType(MacroType::DSP) => output.push_str("│ D "),
                    FPGALayoutType::MacroType(MacroType::BRAM) => output.push_str("│ B "),
                    FPGALayoutType::MacroType(MacroType::IO) => output.push_str("│ I "),
                    FPGALayoutType::EMPTY => output.push_str("│   "),
                }
            }
            output.push_str("│\n");

            // Draw the line below the cells if we are not at the last row
            if y < self.height - 1 {
                for x in 0..self.width {
                    if x == 0 {
                        output.push_str("├───");
                    } else {
                        output.push_str("┼───");
                    }
                }
                output.push_str("┤\n");
            }
        }

        // Draw the bottom line
        for x in 0..self.width {
            if x == 0 {
                output.push_str("└───");
            } else {
                output.push_str("┴───");
            }
        }
        output.push_str("┘\n");

        output
    }
}

pub fn build_simple_fpga_layout(width: u32, height: u32) -> FPGALayout {
    let mut layout = FPGALayout::new(width, height);

    layout.config_border(FPGALayoutType::MacroType(MacroType::IO));
    layout.config_corners(FPGALayoutType::EMPTY);
    layout.config_repeat(
        1,
        1,
        width - 2,
        height - 2,
        1,
        1,
        FPGALayoutType::MacroType(MacroType::CLB),
    );

    // evey 10 columns should be BRAM
    layout.config_repeat(
        10,
        1,
        width - 2,
        height - 2,
        10,
        1,
        FPGALayoutType::MacroType(MacroType::BRAM),
    );

    assert!(layout.valid());

    layout
}
