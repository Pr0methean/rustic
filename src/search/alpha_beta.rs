/* =======================================================================
Rustic is a chess playing engine.
Copyright (C) 2019-2020, Marcel Vanthoor

Rustic is written in the Rust programming language. It is an original
work, not derived from any engine that came before it. However, it does
use a lot of concepts which are well-known and are in use by most if not
all classical alpha/beta-based chess engines.

Rustic is free software: you can redistribute it and/or modify it under
the terms of the GNU General Public License version 3 as published by
the Free Software Foundation.

Rustic is distributed in the hope that it will be useful, but WITHOUT
ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or
FITNESS FOR A PARTICULAR PURPOSE.  See the GNU General Public License
for more details.

You should have received a copy of the GNU General Public License along
with this program.  If not, see <http://www.gnu.org/licenses/>.
======================================================================= */

use super::{
    defs::{SearchTerminate, CHECKMATE, DRAW, STALEMATE},
    Search, SearchRefs,
};
use crate::{
    defs::MAX_DEPTH,
    evaluation,
    movegen::defs::{Move, MoveList, MoveType},
};

impl Search {
    pub fn alpha_beta(depth: u8, mut alpha: i16, beta: i16, refs: &mut SearchRefs) -> i16 {
        // Check if termination condition is met.
        if Search::is_checkpoint(refs) {
            Search::check_for_termination(refs);
        }

        // We have arrived at the leaf node. Evaluate the position and
        // return the result.
        if depth == 0 {
            return Search::quiescence(alpha, beta, refs);
        }

        // Stop going deeper if we hit MAX_DEPTH.
        if refs.search_info.ply >= MAX_DEPTH {
            return evaluation::evaluate_position(refs.board);
        }

        // Temporary variables.
        let mut best_move = Move::new(0);
        let start_alpha = alpha;

        // Generate the moves in this position
        let mut legal_moves_found = 0;
        let mut move_list = MoveList::new();
        refs.mg
            .generate_moves(refs.board, &mut move_list, MoveType::All);

        // Do move scoring, so the best move will be searched first.
        Search::score_moves(&mut move_list);

        // We created a new node which we'll search, so count it.
        refs.search_info.nodes += 1;

        // Iterate over the moves.
        for i in 0..move_list.len() {
            if refs.search_info.terminate != SearchTerminate::Nothing {
                break;
            }

            // This function finds the best move to test according to the
            // move scoring, and puts it at the current index of the move
            // list, so get_move() will get this next.
            Search::pick_move(&mut move_list, i);

            let current_move = move_list.get_move(i);
            let is_legal = refs.board.make(current_move, refs.mg);

            // If not legal, skip the move and the rest of the function.
            if !is_legal {
                continue;
            }

            // Send currently researched move, but only when we are still
            // at the root. This is before we update legal move count, ply,
            // and then recurse deeper.
            if Search::is_root(refs) {
                Search::send_updated_current_move(refs, current_move, legal_moves_found);
            }

            // Send current search stats after a certain number of nodes
            // has been searched.
            if Search::is_update_stats(refs) {
                Search::send_updated_stats(refs);
            }

            // We found a legal move.
            legal_moves_found += 1;
            refs.search_info.ply += 1;

            //We just made a move. We are not yet at one of the leaf nodes,
            //so we must search deeper. We do this by calling alpha/beta
            //again to go to the next ply, but ONLY if this move is NOT
            //causing a draw by repetition or 50-move rule. If it is, we
            //don't have to search anymore: we can just assign DRAW as the
            //eval_score.
            let eval_score = if !Search::is_draw(refs) {
                -Search::alpha_beta(depth - 1, -beta, -alpha, refs)
            } else {
                DRAW
            };

            // Take back the move, and decrease ply accordingly.
            refs.board.unmake();
            refs.search_info.ply -= 1;

            // Beta-cut-off. We return this score, because searching any
            // further down this path would make the situation worse for us
            // and better for our opponent. This is called "fail-high".
            if eval_score >= beta {
                return beta;
            }

            // We found a better move for us.
            if eval_score > alpha {
                // Save our better evaluation score.
                alpha = eval_score;
                best_move = current_move;
            }
        }

        // If we exit the loop without legal moves being found, the
        // side to move is either in checkmate or stalemate.
        if legal_moves_found == 0 {
            let king_square = refs.board.king_square(refs.board.us());
            let opponent = refs.board.opponent();
            let check = refs.mg.square_attacked(refs.board, opponent, king_square);

            if check {
                // The return value is minus CHECKMATE (negative), because
                // if we have no legal moves AND are in check, we have
                // lost. This is a very negative outcome.
                return -CHECKMATE + (refs.search_info.ply as i16);
            } else {
                return STALEMATE;
            }
        }

        // Alpha was improved while walking through the move list, so a
        // better move was found.
        if alpha != start_alpha {
            refs.search_info.best_move = best_move;
        }

        // We have traversed the entire move list and found the best
        // possible move/eval_score for us at this depth. We can't improve
        // this any further, so return the result. This called "fail-low".
        alpha
    }
}
