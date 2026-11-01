// Mastery XP is 2500 * rank^2 through MR 30, then 147,500 per legendary rank.

const MASTERY_XP_PER_RANK_SQUARED = 2_500;
const MASTERY_XP_AT_RANK_30 = MASTERY_XP_PER_RANK_SQUARED * 30 * 30;
const LEGENDARY_RANK_XP = 147_500;

export function masteryXpToRank(xp: number): number {
  if (xp >= MASTERY_XP_AT_RANK_30) {
    return 30 + Math.floor((xp - MASTERY_XP_AT_RANK_30) / LEGENDARY_RANK_XP);
  }
  return Math.floor(Math.sqrt(xp / MASTERY_XP_PER_RANK_SQUARED));
}
