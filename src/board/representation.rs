/**
 * board.rs holds Rustic's board representation and functions associated with it.
 * Rustic uses bitboards. This means there will be at least 6 bitboards for each side;
 * one bitboard per piece type per side.
 * In addition, there are also bitboards containing all white pieces, all black pieces
 * (so it isn't necessary to loop through the bitboards all the time), and bitboards masking
 * files or ranks. Later, more bitboards (diagonals, for exmple) may be added.
 * All other things making up a chess position such as color, castling rights, e_passant
 * and others, will also be in this struct.
*/
use super::fen;
use super::{create_bb_files, create_bb_ranks};
use crate::defs::{
    Bitboard, Piece, Side, BB_FOR_FILES, BB_FOR_RANKS, BITBOARDS_FOR_PIECES, BITBOARDS_PER_SIDE,
    BLACK, FEN_START_POSITION, PNONE, WHITE,
};

pub struct Board {
    pub bb_w: [Bitboard; BITBOARDS_PER_SIDE as usize],
    pub bb_b: [Bitboard; BITBOARDS_PER_SIDE as usize],
    pub bb_pieces: [Bitboard; BITBOARDS_FOR_PIECES as usize],
    pub bb_files: [Bitboard; BB_FOR_FILES as usize],
    pub bb_ranks: [Bitboard; BB_FOR_RANKS as usize],
    pub active_color: u8,
    pub castling: u8,
    pub en_passant: Option<u8>,
    pub halfmove_clock: u8,
    pub fullmove_number: u16,
}

impl Board {
    /**
     * This function creates a new board. If an FEN-position is passed, then use that for
     * setting up the board. If None is passed, use the normal starting position.
     */
    pub fn new(fen: Option<&str>) -> Board {
        let mut board = Board {
            bb_w: [0; BITBOARDS_PER_SIDE as usize],
            bb_b: [0; BITBOARDS_PER_SIDE as usize],
            bb_pieces: [0; BITBOARDS_FOR_PIECES as usize],
            bb_files: [0; BB_FOR_FILES as usize],
            bb_ranks: [0; BB_FOR_RANKS as usize],
            active_color: WHITE as u8,
            castling: 0,
            en_passant: None,
            halfmove_clock: 0,
            fullmove_number: 0,
        };
        board.bb_files = create_bb_files();
        board.bb_ranks = create_bb_ranks();
        if let Some(f) = fen {
            board.setup_fen(f);
        } else {
            board.setup_fen(FEN_START_POSITION);
        }
        board
    }

    /** Reset the board. */
    pub fn reset(&mut self) {
        self.bb_w = [0; BITBOARDS_PER_SIDE as usize];
        self.bb_b = [0; BITBOARDS_PER_SIDE as usize];
        self.bb_pieces = [0; BITBOARDS_FOR_PIECES as usize];
        self.active_color = WHITE as u8;
        self.castling = 0;
        self.en_passant = None;
        self.halfmove_clock = 0;
        self.fullmove_number = 0;
    }

    pub fn setup_fen(&mut self, fen: &str) {
        fen::read(fen, self);
        self.create_piece_bitboards();
    }

    /** Get the pieces of a certain type, for one of the sides. */
    pub fn get_pieces(&self, piece: Piece, side: Side) -> Bitboard {
        debug_assert!(piece <= 5, "Not a piece: {}", piece);
        debug_assert!(side == 0 || side == 1, "Not a side: {}", side);
        match side {
            WHITE => self.bb_w[piece],
            BLACK => self.bb_b[piece],
            _ => 0,
        }
    }

    /** Return which piece is on a given square, or return PNONE (no piece) */
    pub fn which_piece(&self, square: u8) -> Piece {
        debug_assert!(square < 64, "Not a correct square number: {}", square);
        let inspect = 1u64 << square as u64;
        for (piece, (white, black)) in self.bb_w.iter().zip(self.bb_b.iter()).enumerate() {
            if (*white & inspect > 0) || (*black & inspect > 0) {
                return piece;
            }
        }
        PNONE
    }

    /** Return a bitboard containing all the pieces on the board. */
    pub fn occupancy(&self) -> Bitboard {
        self.bb_pieces[WHITE] ^ self.bb_pieces[BLACK]
    }

    /**
     * This function iterates through all the white and black bitboards
     * to create the bitboard holding all of the pieces of that color.
     */
    fn create_piece_bitboards(&mut self) {
        for (bb_w, bb_b) in self.bb_w.iter().zip(self.bb_b.iter()) {
            self.bb_pieces[WHITE] |= bb_w;
            self.bb_pieces[BLACK] |= bb_b;
        }
    }
}
