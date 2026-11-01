import { fetchItemOrderBookBySlug } from "../wfm/orderBook";

interface BookTarget {
  slug: string;
  rank: number | null;
  subtype: string | null;
}

/** The books for one row; a relic row reads only its own refinement's orders. */
export async function fetchRowBook(target: BookTarget) {
  const result = await fetchItemOrderBookBySlug(target.slug, {
    rank: target.rank,
    subtype: target.subtype,
  });
  return result.status === "ok" ? { sell: result.data.sell, buy: result.data.buy } : null;
}
