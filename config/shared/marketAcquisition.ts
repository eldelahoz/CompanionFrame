/** How DE offers an item's blueprint outside the drop tables. Facts live on the
 *  recipe, so no inventory payload confirms them; the item database stamps them. */
export interface MarketAcquisition {
  /** DE offers this item's blueprint in the in-game Market for credits. */
  marketBuyable?: true;
  /** Market credit price of that blueprint; absent when DE lists none. */
  marketCredits?: number;
  /** The blueprint is a clan dojo research project. */
  dojoResearch?: true;
}
