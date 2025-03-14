use crate::board::Board;
use crate::board::defs::{Pieces, BB_RANKS};
use crate::defs::{Bitboard, Sides};

/// A hash function that monotonically decreases whenever a pawn moves, a piece is captured, or a
/// player castles or gives up the right to castle. Used to quickly eliminate unreachable positions
/// from the transposition table.
impl Board {
    pub fn monotonic_hash(&self) -> u32 {
        // Decreasing multipliers for stronger pieces
        // Each side can have 0..=8 pawns and usually has 0..=1 or 0..=2 of each other piece,
        // which works out to only 419903 reasonably likely values for each side. But "unreasonable"
        // numbers of all pieces except pawns can happen because of underpromotion, so we round up
        // to the next prime.
        // The maximum is 46691*8+5189*8+1733*2+577*2+293+149+73+37+13*2+5*2+3+1 = 420252.
        const PAWN_MULT: [u32; 2] = [5189, 46691]; // usually 0..=8; raw: [5184, 46656]
        const KNIGHT_MULT: [u32; 2] = [1733, 577]; // usually 0..=2; raw: [1728, 576]
        const LIGHT_BISHOP_MULT: [u32; 2] = [149, 293]; // usually 0..=1; raw: [144, 288]
        const DARK_BISHOP_MULT: [u32; 2] = [73, 37]; // usually 0..=1; raw: [72, 36]
        const ROOK_MULT: [u32; 2] = [5, 13]; // usually 0..=2; raw: [4,12]
        const QUEEN_MULT: [u32; 2] = [3, 1]; // usually 0..=1; raw: [2,1]

        const DARK_SQUARES: Bitboard = 0xAA55AA55AA55AA55;
        const LIGHT_SQUARES: Bitboard = !DARK_SQUARES;
        let mut pieces_keys = [0u32; 2];

        let white_pawns = self.get_pieces(Pieces::PAWN, Sides::WHITE);
        let black_pawns = self.get_pieces(Pieces::PAWN, Sides::BLACK);
        for side in [Sides::WHITE, Sides::BLACK] {
            let pawns = [white_pawns, black_pawns][side as usize].count_ones();
            let knights = self.get_pieces(Pieces::KNIGHT, side).count_ones();
            let bishops = self.get_pieces(Pieces::BISHOP, side);
            let light_bishops = (bishops & LIGHT_SQUARES).count_ones();
            let dark_bishops = (bishops & DARK_SQUARES).count_ones();
            let rooks = self.get_pieces(Pieces::ROOK, side).count_ones();
            let queens = self.get_pieces(Pieces::QUEEN, side).count_ones();

            // Simply add weighted piece counts
            pieces_keys[side as usize] =
                pawns * PAWN_MULT[side as usize] +
                    knights * KNIGHT_MULT[side as usize] +
                    light_bishops * LIGHT_BISHOP_MULT[side as usize] +
                    dark_bishops * DARK_BISHOP_MULT[side as usize] +
                    rooks * ROOK_MULT[side as usize] +
                    queens * QUEEN_MULT[side as usize];
        }

        // Maximum castling key is 420253 * 15 because only the lower 4 bits are used.
        let castling_key = self.game_state.castling as u32 * 420253;

        // Use multipliers for rank weights
        // Largest possible rank value is 255 for each side
        // and the piece and castling keys leave a maximum value of 4288243248 for the pawn key,
        // so the sum of the 2 largest multipliers should be less than 4288243248/255.
        // The ones used here are the nearest primes to powers of the positive root of
        // x.pow(10) + x.pow(11) == 4288243248.0/255.0, which is
        // 4.455443274968434891800999910616226042276, excluding primes already used as multipliers
        // above and adjusting the largest few to minimize the residual.
        let mut pawns_key = 0u32;
        const WHITE_RANK_MULTIPLIERS: [u32; 6] = [13734121, 155291, 34849, 397, 89, 2];
        const BLACK_RANK_MULTIPLIERS: [u32; 6] = [3082517, 691878, 7823, 1753, 19, 7];
        for ranks_advanced in 0..=5 {
            let white_pawn_rank_key = (white_pawns & BB_RANKS[1 + ranks_advanced]) as u32
                * WHITE_RANK_MULTIPLIERS[ranks_advanced];
            let black_pawn_rank_key = (black_pawns & BB_RANKS[6 - ranks_advanced]) as u32
                * BLACK_RANK_MULTIPLIERS[ranks_advanced];
            pawns_key += white_pawn_rank_key + black_pawn_rank_key;
        }

        pawns_key + pieces_keys[0] + pieces_keys[1] + castling_key
    }
}


