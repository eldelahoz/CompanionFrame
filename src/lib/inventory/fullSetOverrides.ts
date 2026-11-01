interface FullSetOverrideComponent {
  name: string;
  uniqueName: string;
  itemCount?: number;
}

interface FullSetOverride {
  slug: string;
  rootUniqueName: string;
  rootName?: string;
  components: readonly FullSetOverrideComponent[];
}

// Exact components for market sets that the item database cannot reconstruct.
export const FULL_SET_OVERRIDES: readonly FullSetOverride[] = [
  {
    slug: "mantis_set",
    rootUniqueName: "/Lotus/Types/Items/Ships/InsectShip",
    components: [
      {
        name: "Mantis Avionics Blueprint",
        uniqueName: "/Lotus/Types/Recipes/LandingCraftRecipes/Mantys/MantysStarChartBlueprint",
      },
      {
        name: "Mantis Engines Blueprint",
        uniqueName: "/Lotus/Types/Recipes/LandingCraftRecipes/Mantys/MantysPowerCoreBlueprint",
      },
      {
        name: "Mantis Fuselage Blueprint",
        uniqueName: "/Lotus/Types/Recipes/LandingCraftRecipes/Mantys/MantysExoskeletonBlueprint",
      },
    ],
  },
  {
    slug: "kavasa_prime_kubrow_collar_set",
    rootUniqueName: "/Lotus/Upgrades/Skins/Kubrows/Collars/PrimeKubrowCollarA",
    components: [
      {
        name: "Kavasa Prime Band",
        uniqueName: "/Lotus/Types/Recipes/Kubrow/Collars/PrimeKubrowCollarABandComponent",
      },
      {
        name: "Kavasa Prime Buckle",
        uniqueName: "/Lotus/Types/Recipes/Kubrow/Collars/PrimeKubrowCollarABuckleComponent",
      },
      {
        name: "Kavasa Prime Kubrow Collar Blueprint",
        uniqueName: "/Lotus/Types/Recipes/Kubrow/Collars/PrimeKubrowCollarABlueprint",
      },
    ],
  },
  {
    slug: "scimitar_set",
    rootUniqueName: "/Lotus/Types/Items/Ships/BlueSkyShip",
    components: [
      {
        name: "Scimitar Avionics Blueprint",
        uniqueName: "/Lotus/Types/Recipes/LandingCraftRecipes/BlueSky/BlueSkyAvionicsBlueprint",
      },
      {
        name: "Scimitar Engines Blueprint",
        uniqueName: "/Lotus/Types/Recipes/LandingCraftRecipes/BlueSky/BlueSkyEnginesBlueprint",
      },
      {
        name: "Scimitar Fuselage Blueprint",
        uniqueName: "/Lotus/Types/Recipes/LandingCraftRecipes/BlueSky/BlueSkyFuselageBlueprint",
      },
    ],
  },
  {
    slug: "amesha_set",
    rootUniqueName: "/Lotus/Powersuits/Archwing/SupportJetPack/SupportJetPack",
    components: [
      {
        name: "Amesha Systems",
        uniqueName:
          "/Lotus/Types/Recipes/ArchwingRecipes/SupportArchwing/SupportArchwingSystemsComponent",
      },
      {
        name: "Amesha Wings",
        uniqueName:
          "/Lotus/Types/Recipes/ArchwingRecipes/SupportArchwing/SupportArchwingWingsComponent",
      },
      {
        name: "Amesha Harness",
        uniqueName:
          "/Lotus/Types/Recipes/ArchwingRecipes/SupportArchwing/SupportArchwingChassisComponent",
      },
    ],
  },
  {
    slug: "elytron_set",
    rootUniqueName: "/Lotus/Powersuits/Archwing/DemolitionJetPack/DemolitionJetPack",
    components: [
      {
        name: "Elytron Systems",
        uniqueName:
          "/Lotus/Types/Recipes/ArchwingRecipes/DemolitionArchwing/DemolitionArchwingSystemsComponent",
      },
      {
        name: "Elytron Wings",
        uniqueName:
          "/Lotus/Types/Recipes/ArchwingRecipes/DemolitionArchwing/DemolitionArchwingWingsComponent",
      },
      {
        name: "Elytron Harness",
        uniqueName:
          "/Lotus/Types/Recipes/ArchwingRecipes/DemolitionArchwing/DemolitionArchwingChassisComponent",
      },
    ],
  },
  {
    slug: "itzal_set",
    rootUniqueName: "/Lotus/Powersuits/Archwing/StealthJetPack/StealthJetPack",
    components: [
      {
        name: "Itzal Harness Blueprint",
        uniqueName:
          "/Lotus/Types/Recipes/ArchwingRecipes/StealthArchwing/StealthArchwingChassisBlueprint",
      },
      {
        name: "Itzal Systems Blueprint",
        uniqueName:
          "/Lotus/Types/Recipes/ArchwingRecipes/StealthArchwing/StealthArchwingSystemsBlueprint",
      },
      {
        name: "Itzal Wings Blueprint",
        uniqueName:
          "/Lotus/Types/Recipes/ArchwingRecipes/StealthArchwing/StealthArchwingWingsBlueprint",
      },
    ],
  },
  {
    slug: "xiphos_set",
    rootUniqueName: "/Lotus/Types/Items/Ships/GyroscopeShip",
    components: [
      {
        name: "Xiphos Avionics Blueprint",
        uniqueName: "/Lotus/Types/Recipes/LandingCraftRecipes/Gyroscope/GyroscopeAvionicsBlueprint",
      },
      {
        name: "Xiphos Engines Blueprint",
        uniqueName: "/Lotus/Types/Recipes/LandingCraftRecipes/Gyroscope/GyroscopeEnginesBlueprint",
      },
      {
        name: "Xiphos Fuselage Blueprint",
        uniqueName: "/Lotus/Types/Recipes/LandingCraftRecipes/Gyroscope/GyroscopeFuselageBlueprint",
      },
    ],
  },
  {
    slug: "bonewidow_set",
    rootUniqueName: "/Lotus/Powersuits/EntratiMech/ThanoTech",
    components: [
      {
        name: "Bonewidow Capsule",
        uniqueName:
          "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/ThanomechPartSystemsItem",
      },
      {
        name: "Bonewidow Casing",
        uniqueName:
          "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/ThanomechPartChassisItem",
      },
      {
        name: "Bonewidow Engine",
        uniqueName:
          "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/ThanomechPartEngineItem",
      },
      {
        name: "Bonewidow Weapon Pod",
        uniqueName:
          "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/ThanomechPartWeaponPodItem",
      },
    ],
  },
  {
    slug: "cortege_set",
    rootUniqueName: "/Lotus/Weapons/Tenno/Archwing/Primary/ThanoTechArchGun/ThanoTechArchGun",
    components: [
      {
        name: "Cortege Barrel",
        uniqueName:
          "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/ThanotechArchGunBarrelItem",
      },
      {
        name: "Cortege Receiver",
        uniqueName:
          "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/ThanotechArchGunReceiverItem",
      },
      {
        name: "Cortege Stock",
        uniqueName:
          "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/ThanotechArchGunStockItem",
      },
    ],
  },
  {
    slug: "voidrig_set",
    rootUniqueName: "/Lotus/Powersuits/EntratiMech/NechroTech",
    components: [
      {
        name: "Voidrig Capsule",
        uniqueName:
          "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/NecromechPartSystemsItem",
      },
      {
        name: "Voidrig Casing",
        uniqueName:
          "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/NecromechPartChassisItem",
      },
      {
        name: "Voidrig Engine",
        uniqueName:
          "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/NecromechPartEngineItem",
      },
      {
        name: "Voidrig Weapon Pod",
        uniqueName:
          "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/NecromechPartWeaponPodItem",
      },
    ],
  },
  {
    slug: "morgha_set",
    rootUniqueName:
      "/Lotus/Weapons/Tenno/Archwing/Primary/ThanoTechGrenadeLaunch/ThanoTechGrenadeLauncher",
    components: [
      {
        name: "Morgha Barrel",
        uniqueName:
          "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/ThanotechGrenadeLauncherBarrelItem",
      },
      {
        name: "Morgha Receiver",
        uniqueName:
          "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/ThanotechGrenadeLauncherReceiverItem",
      },
      {
        name: "Morgha Stock",
        uniqueName:
          "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/ThanotechGrenadeLauncherStockItem",
      },
    ],
  },
  {
    slug: "parallax_set",
    rootUniqueName: "/Lotus/Types/Items/Ships/ZarimanShip",
    components: [
      {
        name: "Parallax Avionics Blueprint",
        uniqueName:
          "/Lotus/Types/Recipes/LandingCraftRecipes/ZarimanShip/ZarimanShipAvionicsBlueprint",
      },
      {
        name: "Parallax Engines Blueprint",
        uniqueName:
          "/Lotus/Types/Recipes/LandingCraftRecipes/ZarimanShip/ZarimanShipEnginesBlueprint",
      },
      {
        name: "Parallax Fuselage Blueprint",
        uniqueName:
          "/Lotus/Types/Recipes/LandingCraftRecipes/ZarimanShip/ZarimanShipFuselageBlueprint",
      },
    ],
  },
  {
    slug: "prisma_shade_set",
    rootUniqueName: "/Lotus/Types/Sentinels/SentinelPowersuits/PrismaShadePowerSuit",
    components: [
      {
        name: "Prisma Burst Laser",
        uniqueName: "/Lotus/Types/Sentinels/SentinelWeapons/PrismaBurstLaserPistol",
      },
      {
        name: "Prisma Shade",
        uniqueName: "/Lotus/Types/Sentinels/SentinelPowersuits/PrismaShadePowerSuit",
      },
    ],
  },
  {
    slug: "nautilus_set",
    rootUniqueName: "/Lotus/Types/Sentinels/SentinelPowersuits/EmpyreanSentinelPowerSuit",
    components: [
      {
        name: "Nautilus Cerebrum",
        uniqueName: "/Lotus/Types/Recipes/Weapons/WeaponParts/EmpyreanSentinelCerebrum",
      },
      {
        name: "Nautilus Blueprint",
        uniqueName: "/Lotus/Types/Recipes/SentinelRecipes/EmpyreanSentinelBlueprint",
      },
      {
        name: "Nautilus Carapace",
        uniqueName: "/Lotus/Types/Recipes/Weapons/WeaponParts/EmpyreanSentinelCarapace",
      },
      {
        name: "Nautilus Systems",
        uniqueName: "/Lotus/Types/Recipes/Weapons/WeaponParts/EmpyreanSentinelSystems",
      },
    ],
  },
  {
    slug: "styanax_prime_set",
    rootUniqueName: "/Lotus/Powersuits/Hoplite/StyanaxPrime",
    components: [
      {
        name: "Styanax Prime Blueprint",
        uniqueName: "/Lotus/Types/Recipes/WarframeRecipes/StyanaxPrimeBlueprint",
      },
      {
        name: "Styanax Prime Chassis Blueprint",
        uniqueName: "/Lotus/Types/Recipes/WarframeRecipes/StyanaxPrimeChassisBlueprint",
      },
      {
        name: "Styanax Prime Neuroptics Blueprint",
        uniqueName: "/Lotus/Types/Recipes/WarframeRecipes/StyanaxPrimeHelmetBlueprint",
      },
      {
        name: "Styanax Prime Systems Blueprint",
        uniqueName: "/Lotus/Types/Recipes/WarframeRecipes/StyanaxPrimeSystemsBlueprint",
      },
    ],
  },
  {
    slug: "afentis_prime_set",
    rootUniqueName: "/Lotus/Weapons/Tenno/LongGuns/PrimeAfentis/PrimeAfentisWeapon",
    components: [
      {
        name: "Afentis Prime Barrel",
        uniqueName: "/Lotus/Types/Recipes/Weapons/WeaponParts/AfentisPrimeBarrel",
      },
      {
        name: "Afentis Prime Blade",
        uniqueName: "/Lotus/Types/Recipes/Weapons/WeaponParts/AfentisPrimeBlade",
      },
      {
        name: "Afentis Prime Blueprint",
        uniqueName: "/Lotus/Types/Recipes/Weapons/AfentisPrimeBlueprint",
      },
      {
        name: "Afentis Prime Handle",
        uniqueName: "/Lotus/Types/Recipes/Weapons/WeaponParts/AfentisPrimeHandle",
      },
    ],
  },
  {
    slug: "athodai_prime_set",
    rootUniqueName: "/Lotus/Weapons/Tenno/Pistols/PrimeAthodai/PrimeAthodaiPistolWeapon",
    components: [
      {
        name: "Athodai Prime Barrel",
        uniqueName: "/Lotus/Types/Recipes/Weapons/WeaponParts/AthodaiPrimeBarrel",
      },
      {
        name: "Athodai Prime Blueprint",
        uniqueName: "/Lotus/Types/Recipes/Weapons/AthodaiPrimeBlueprint",
      },
      {
        name: "Athodai Prime Receiver",
        uniqueName: "/Lotus/Types/Recipes/Weapons/WeaponParts/AthodaiPrimeReceiver",
      },
    ],
  },
  {
    slug: "damaged_necramech_set",
    rootUniqueName: "/WFHelper/Sets/DamagedNecramech",
    rootName: "Damaged Necramech",
    components: [
      {
        name: "Damaged Necramech Casing",
        uniqueName:
          "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/DamagedMechPartChassisItem",
      },
      {
        name: "Damaged Necramech Engine",
        uniqueName:
          "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/DamagedMechPartEngineItem",
      },
      {
        name: "Damaged Necramech Pod",
        uniqueName:
          "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/DamagedMechPartSystemsItem",
      },
      {
        name: "Damaged Necramech Weapon Pod",
        uniqueName:
          "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/DamagedMechPartWeaponPodItem",
      },
    ],
  },
  {
    slug: "damaged_necramech_weapon_set",
    rootUniqueName: "/WFHelper/Sets/DamagedNecramechWeapon",
    rootName: "Damaged Necramech Weapon",
    components: [
      {
        name: "Damaged Necramech Weapon Barrel",
        uniqueName:
          "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/DamagedMechWeaponBarrelItem",
      },
      {
        name: "Damaged Necramech Weapon Receiver",
        uniqueName:
          "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/DamagedMechWeaponReceiverItem",
      },
      {
        name: "Damaged Necramech Weapon Stock",
        uniqueName:
          "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/DamagedMechWeaponStockItem",
      },
    ],
  },
];

const FULL_SET_OVERRIDE_BY_ROOT = new Map(
  FULL_SET_OVERRIDES.map((entry) => [entry.rootUniqueName, entry] as const),
);

export function getFullSetOverride(rootUniqueName: string): FullSetOverride | undefined {
  return FULL_SET_OVERRIDE_BY_ROOT.get(rootUniqueName);
}
