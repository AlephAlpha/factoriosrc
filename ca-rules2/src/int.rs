//! Tables of the isotropic non-totalistic (INT) classes.

/// The isotropic non-totalistic classes of the range-1 Moore neighborhood,
/// in [Hensel notation](https://conwaylife.com/wiki/Hensel_notation).
///
/// For each number of living neighbors, and for each class letter, the list
/// of 8-bit patterns of living neighbors is given. The `i`-th bit of a pattern
/// corresponds to the `i`-th neighbor in
/// [`NeighborhoodType::neighbor_coords`](crate::NeighborhoodType::neighbor_coords)
/// for the range-1 Moore neighborhood.
///
/// For each count `d`, the classes partition all patterns with exactly `d`
/// living neighbors.
///
/// This table is copied from the `ca-rules` crate.
pub const INT_LIFE_TABLE: [&[(u8, &[u8])]; 9] = [
    &[(b'c', &[0x00])],
    &[
        (b'c', &[0x01, 0x04, 0x20, 0x80]),
        (b'e', &[0x02, 0x08, 0x10, 0x40]),
    ],
    &[
        (b'c', &[0x05, 0x21, 0x84, 0xa0]),
        (b'e', &[0x0a, 0x12, 0x48, 0x50]),
        (b'k', &[0x0c, 0x11, 0x22, 0x30, 0x41, 0x44, 0x82, 0x88]),
        (b'a', &[0x03, 0x06, 0x09, 0x14, 0x28, 0x60, 0x90, 0xc0]),
        (b'i', &[0x18, 0x42]),
        (b'n', &[0x24, 0x81]),
    ],
    &[
        (b'c', &[0x25, 0x85, 0xa1, 0xa4]),
        (b'e', &[0x1a, 0x4a, 0x52, 0x58]),
        (b'k', &[0x32, 0x4c, 0x51, 0x8a]),
        (b'a', &[0x0b, 0x16, 0x68, 0xd0]),
        (b'i', &[0x07, 0x29, 0x94, 0xe0]),
        (b'n', &[0x0d, 0x15, 0x23, 0x61, 0x86, 0xa8, 0xb0, 0xc4]),
        (b'y', &[0x31, 0x45, 0x8c, 0xa2]),
        (b'q', &[0x26, 0x2c, 0x34, 0x64, 0x83, 0x89, 0x91, 0xc1]),
        (b'j', &[0x0e, 0x13, 0x2a, 0x49, 0x54, 0x70, 0x92, 0xc8]),
        (b'r', &[0x19, 0x1c, 0x38, 0x43, 0x46, 0x62, 0x98, 0xc2]),
    ],
    &[
        (b'c', &[0xa5]),
        (b'e', &[0x5a]),
        (b'k', &[0x33, 0x4d, 0x55, 0x71, 0x8e, 0xaa, 0xb2, 0xcc]),
        (b'a', &[0x0f, 0x17, 0x2b, 0x69, 0x96, 0xd4, 0xe8, 0xf0]),
        (b'i', &[0x1d, 0x63, 0xb8, 0xc6]),
        (b'n', &[0x27, 0x2d, 0x87, 0x95, 0xa9, 0xb4, 0xe1, 0xe4]),
        (b'y', &[0x35, 0x65, 0x8d, 0xa3, 0xa6, 0xac, 0xb1, 0xc5]),
        (b'q', &[0x36, 0x6c, 0x8b, 0xd1]),
        (b'j', &[0x3a, 0x4e, 0x53, 0x59, 0x5c, 0x72, 0x9a, 0xca]),
        (b'r', &[0x1b, 0x1e, 0x4b, 0x56, 0x6a, 0x78, 0xd2, 0xd8]),
        (b't', &[0x39, 0x47, 0x9c, 0xe2]),
        (b'w', &[0x2e, 0x74, 0x93, 0xc9]),
        (b'z', &[0x3c, 0x66, 0x99, 0xc3]),
    ],
    &[
        (b'c', &[0x5b, 0x5e, 0x7a, 0xda]),
        (b'e', &[0xa7, 0xad, 0xb5, 0xe5]),
        (b'k', &[0x75, 0xae, 0xb3, 0xcd]),
        (b'a', &[0x2f, 0x97, 0xe9, 0xf4]),
        (b'i', &[0x1f, 0x6b, 0xd6, 0xf8]),
        (b'n', &[0x3b, 0x4f, 0x57, 0x79, 0x9e, 0xdc, 0xea, 0xf2]),
        (b'y', &[0x5d, 0x73, 0xba, 0xce]),
        (b'q', &[0x3e, 0x6e, 0x76, 0x7c, 0x9b, 0xcb, 0xd3, 0xd9]),
        (b'j', &[0x37, 0x6d, 0x8f, 0xab, 0xb6, 0xd5, 0xec, 0xf1]),
        (b'r', &[0x3d, 0x67, 0x9d, 0xb9, 0xbc, 0xc7, 0xe3, 0xe6]),
    ],
    &[
        (b'c', &[0x5f, 0x7b, 0xde, 0xfa]),
        (b'e', &[0xaf, 0xb7, 0xed, 0xf5]),
        (b'k', &[0x77, 0x7d, 0xbb, 0xbe, 0xcf, 0xdd, 0xee, 0xf3]),
        (b'a', &[0x3f, 0x6f, 0x9f, 0xd7, 0xeb, 0xf6, 0xf9, 0xfc]),
        (b'i', &[0xbd, 0xe7]),
        (b'n', &[0x7e, 0xdb]),
    ],
    &[
        (b'c', &[0x7f, 0xdf, 0xfb, 0xfe]),
        (b'e', &[0xbf, 0xef, 0xf7, 0xfd]),
    ],
    &[(b'c', &[0xff])],
];

/// The isotropic non-totalistic classes of the range-1 hexagonal neighborhood,
/// emulated on a square grid.
///
/// The meaning of the letters is the same as in
/// [hexagonal INT rules](https://conwaylife.com/wiki/Isotropic_non-totalistic_rule#Hexagonal_grid):
/// `o` means the neighbors form a connected arc, `m` means that they are
/// separated by one cell, and `p` means that they are opposite to each other.
///
/// For each number of living neighbors, and for each class letter, the list
/// of 6-bit patterns of living neighbors is given. The `i`-th bit of a pattern
/// corresponds to the `i`-th neighbor in
/// [`NeighborhoodType::neighbor_coords`](crate::NeighborhoodType::neighbor_coords)
/// for the range-1 hexagonal neighborhood.
///
/// For each count `d`, the classes partition all patterns with exactly `d`
/// living neighbors.
///
/// This table is copied from the `ca-rules` crate.
pub const INT_HEX_TABLE: [&[(u8, &[u8])]; 7] = [
    &[(b'o', &[0x00])],
    &[(b'o', &[0x01, 0x02, 0x04, 0x08, 0x10, 0x20])],
    &[
        (b'o', &[0x03, 0x05, 0x0a, 0x14, 0x28, 0x30]),
        (b'm', &[0x06, 0x09, 0x11, 0x18, 0x22, 0x24]),
        (b'p', &[0x0c, 0x12, 0x21]),
    ],
    &[
        (b'o', &[0x07, 0x0b, 0x15, 0x2a, 0x34, 0x38]),
        (
            b'm',
            &[
                0x0d, 0x0e, 0x13, 0x16, 0x1a, 0x1c, 0x23, 0x25, 0x29, 0x2c, 0x31, 0x32,
            ],
        ),
        (b'p', &[0x19, 0x26]),
    ],
    &[
        (b'o', &[0x0f, 0x17, 0x2b, 0x35, 0x3a, 0x3c]),
        (b'm', &[0x1b, 0x1d, 0x27, 0x2e, 0x36, 0x39]),
        (b'p', &[0x1e, 0x2d, 0x33]),
    ],
    &[(b'o', &[0x1f, 0x2f, 0x37, 0x3b, 0x3d, 0x3e])],
    &[(b'o', &[0x3f])],
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NeighborhoodType;
    use ca_symmetry::Transformation;

    /// Apply a transformation to a mask of neighbors, using the given
    /// coordinates of the neighbors.
    fn apply_to_mask(mask: u8, coords: &[(i32, i32)], t: Transformation) -> u8 {
        let mut result = 0;
        for (i, &(x, y)) in coords.iter().enumerate() {
            if mask & (1 << i) != 0 {
                let (x2, y2) = t.apply(x, y);
                let j = coords
                    .iter()
                    .position(|&(x3, y3)| (x3, y3) == (x2, y2))
                    .expect("the transformation preserves the neighborhood");
                result |= 1 << j;
            }
        }
        result
    }

    /// Check that a table of classes is well-formed:
    /// - for each count, the classes partition the patterns with exactly that
    ///   many living neighbors;
    /// - each class is closed under the given transformations.
    fn check_table(table: &[&[(u8, &[u8])]], size: u8, symmetries: &[Transformation]) {
        let coords = if size == 8 {
            NeighborhoodType::Moore.neighbor_coords(1)
        } else {
            NeighborhoodType::Hexagonal.neighbor_coords(1)
        };

        for (digit, classes) in table.iter().enumerate() {
            let mut remaining: Vec<u8> = (0..1 << size)
                .map(|mask| mask as u8)
                .filter(|&mask| mask.count_ones() as usize == digit)
                .collect();

            for &(letter, masks) in *classes {
                assert!(
                    letter.is_ascii_lowercase(),
                    "the class letter must be lowercase"
                );
                for &mask in masks {
                    assert_eq!(
                        mask.count_ones() as usize,
                        digit,
                        "the pattern {mask:#04x} has the wrong number of living neighbors"
                    );
                    let position = remaining
                        .iter()
                        .position(|&m| m == mask)
                        .unwrap_or_else(|| {
                            panic!(
                                "the pattern {mask:#04x} appears in more than one class of {digit}"
                            )
                        });
                    remaining.remove(position);
                }
            }

            assert!(
                remaining.is_empty(),
                "missing patterns for the count {digit}: {remaining:#04x?}"
            );

            for &(letter, masks) in *classes {
                for &mask in masks {
                    for &t in symmetries {
                        let transformed = apply_to_mask(mask, &coords, t);
                        assert!(
                            masks.contains(&transformed),
                            "the pattern {mask:#04x} of the class {digit}{} is not invariant under {t:?}",
                            letter as char
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_int_life_table() {
        let symmetries = Transformation::iter().collect::<Vec<_>>();
        check_table(&INT_LIFE_TABLE, 8, &symmetries);
    }

    #[test]
    fn test_int_hex_table() {
        // The hexagonal neighborhood is only invariant under R0, R2, S1, and S3.
        let symmetries = [
            Transformation::R0,
            Transformation::R2,
            Transformation::S1,
            Transformation::S3,
        ];
        check_table(&INT_HEX_TABLE, 6, &symmetries);
    }
}
