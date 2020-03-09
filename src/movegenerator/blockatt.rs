/**
 * blockatt.rs is the Blockers/Attacks generation module.
 * It generates all possible blocker boards for a given mask,
 * rook attack boards, and bishop attack boards.
*/
use super::init::{AttackBoards, BlockerBoards};
use super::rays::create_bb_ray;
use crate::definitions::Bitboard;
use crate::utils::Direction;

/**
 * create_blocker_boards() takes a mask. This is a bitboard in which
 * all the bits are set for a square a slider can move to, without the
 * edges. (As generated by the functions in the mask.rs module.)
 * create_blocker_boards() generates all possible permutations for the
 * given mask, using the Carry Rippler method. See the given link, or
 * http://rustic-chess.org for more information.
*/
pub fn create_blocker_boards(mask: Bitboard) -> BlockerBoards {
    let d: Bitboard = mask;
    let mut bb_blocker_boards: BlockerBoards = Vec::new();
    let mut n: Bitboard = 0;

    // Carry-Rippler
    // https://www.chessprogramming.org/Traversing_Subsets_of_a_Set
    loop {
        bb_blocker_boards.push(n);
        n = n.wrapping_sub(d) & d;
        if n == 0 {
            break;
        }
    }

    bb_blocker_boards
}

/**
 * This function takes a square, and all the blocker boards belonging to that squre.
 * Then it'll iterate through those blocker boards, and generate the attack board
 * belonging to that blocker board. The 'length' parameter is the length of the given
 * array of blocker boards.
*/
pub fn create_rook_attack_boards(sq: u8, blockers: &[Bitboard]) -> AttackBoards {
    let mut bb_attack_boards: AttackBoards = Vec::new();

    for b in blockers.iter() {
        let bb_attacks = create_bb_ray(*b, sq, Direction::Up)
            | create_bb_ray(*b, sq, Direction::Right)
            | create_bb_ray(*b, sq, Direction::Down)
            | create_bb_ray(*b, sq, Direction::Left);
        bb_attack_boards.push(bb_attacks);
    }

    bb_attack_boards
}

/* Same as the function above, but for the bishop. */
pub fn create_bishop_attack_boards(sq: u8, blockers: &[Bitboard]) -> AttackBoards {
    let mut bb_attack_boards: AttackBoards = Vec::new();

    for b in blockers.iter() {
        let bb_attacks = create_bb_ray(*b, sq, Direction::UpLeft)
            | create_bb_ray(*b, sq, Direction::UpRight)
            | create_bb_ray(*b, sq, Direction::DownRight)
            | create_bb_ray(*b, sq, Direction::DownLeft);
        bb_attack_boards.push(bb_attacks);
    }

    bb_attack_boards
}
