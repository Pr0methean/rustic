use crate::board::Board;
use crate::board::defs::{Pieces, BB_SQUARES};
use crate::defs::{Bitboard, Sides};

impl Board {
    /// A hash function that monotonically decreases whenever a pawn moves, a piece is captured, or a
    /// player castles or gives up the right to castle or capture en passant. Used to quickly eliminate unreachable positions
    /// from the transposition table.
    pub fn monotonic_hash(&self) -> u128 {
        let mut key: u128 = 0;
        const PAWN_SHIFT: [u32; 2] = [5197, 46703]; // usually 0..=8; raw: [5184, 46656]
        const KNIGHT_MULT: [u32; 2] = [1733, 577]; // usually 0..=2; raw: [1728, 576]
        const LIGHT_BISHOP_MULT: [u32; 2] = [149, 293]; // usually 0..=1; raw: [144, 288]
        const DARK_BISHOP_MULT: [u32; 2] = [73, 37]; // usually 0..=1; raw: [72, 36]
        const ROOK_MULT: [u32; 2] = [5, 13]; // usually 0..=2; raw: [4,12]
        const QUEEN_MULT: [u32; 2] = [3, 1]; // usually 0..=1; raw: [2,1]

        const DARK_SQUARES: Bitboard = 0xAA55AA55AA55AA55;
        const LIGHT_SQUARES: Bitboard = !DARK_SQUARES;
        let mut pieces_keys = [0; 2];

        let white_pawns = self.get_pieces(Pieces::PAWN, Sides::WHITE);
        let black_pawns = self.get_pieces(Pieces::PAWN, Sides::BLACK);
        // Max pieces key is 12100 * 9 + 1210 + 121 + 11 * 2 + 2 = 110255, so each pieces key needs 17 bits.
        for side in [Sides::WHITE, Sides::BLACK] {
            let knights = self.get_pieces(Pieces::KNIGHT, side).count_ones();
            let bishops = self.get_pieces(Pieces::BISHOP, side);
            let light_bishops = (bishops & LIGHT_SQUARES).count_ones();
            let dark_bishops = (bishops & DARK_SQUARES).count_ones();
            let rooks = self.get_pieces(Pieces::ROOK, side).count_ones();
            let queens = self.get_pieces(Pieces::QUEEN, side).count_ones();
            pieces_keys[side] = rooks
                + 11 * knights
                + 11 * 11 * light_bishops
                + 11 * 11 * 10 * dark_bishops
                + 11 * 11 * 10 * 10 * queens;
        }
        key |= (pieces_keys[0] as u128) | (pieces_keys[1] as u128) << 17;

        // Castling fits in 4 bits because only the lower 4 bits are used.
        key |= (self.game_state.castling as u128) << 34;

        // En passant fits in 7 bits since en_passant is a square index.
        key |= self.game_state.en_passant.map_or(0, |ep| (ep as u128 + 1) << 38);

        // For each side, we store a combination index of the pawns, reversed so that it
        // decreases as the pawns advance or are captured.
        // This method is based on https://math.stackexchange.com/a/1227692, adapted for the
        // variable number of pawns.
        // We need ceil(log2(TOTAL_COMBINATIONS)) == 58 bits total.
        const fn count_combinations(n: u64, r: u64) -> u64 {
            if n < r {
                return 0;
            }
            let mut combos = 1;
            for i in 1..=r {
                combos *= (n - i + 1) / i;
            }
            combos
        }
        const CHOOSE_OF_48: [u64; 9] = [
            count_combinations(48, 0),
            count_combinations(48, 1),
            count_combinations(48, 2),
            count_combinations(48, 3),
            count_combinations(48, 4),
            count_combinations(48, 5),
            count_combinations(48, 6),
            count_combinations(48, 7),
            count_combinations(48, 8),
        ];
        const TOTAL_COMBINATIONS: u64 = CHOOSE_OF_48.iter().sum();

        let mut white_pawns_left = white_pawns.count_ones() as usize;
        let mut black_pawns_left = black_pawns.count_ones() as usize;
        let mut white_index = 0;
        let mut black_index = 0;
        for captured_or_promoted in (1..=8).rev() {
            if white_pawns_left > captured_or_promoted {
                white_index += CHOOSE_OF_48[white_pawns_left];
            }
            if black_pawns_left > captured_or_promoted {
                black_index += CHOOSE_OF_48[black_pawns_left];
            }
        }
        for square in 8..=55 {
            if white_pawns & BB_SQUARES[64 - square] != 0 {
                white_index += count_combinations((square - 8) as u64, white_pawns_left as u64);
                white_pawns_left -= 1;
            }
            if black_pawns & BB_SQUARES[square] != 0 {
                black_index +=  count_combinations((56 - square) as u64, black_pawns_left as u64);
                black_pawns_left -= 1;
            }
        }
        key |= (((TOTAL_COMBINATIONS * (TOTAL_COMBINATIONS - black_index)) + TOTAL_COMBINATIONS - white_index) as u128) << 45;
        key
    }
}


