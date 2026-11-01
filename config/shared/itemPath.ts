/** Store paths mirror inventory identities under an extra StoreItems segment. */
export function storeItemPath(itemPath: string | undefined): string {
  return (itemPath || "").replace(/^\/Lotus\/StoreItems\//, "/Lotus/");
}
