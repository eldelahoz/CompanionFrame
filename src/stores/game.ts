import { useEffect, useState } from "react";
import { getTrackersStatus, onGameLogin, onGameRunning, type TrackersStatus } from "../lib/ipc";

export type GameState = TrackersStatus["game"];

const IDLE: GameState = { running: false, logged_in: false, mission_tag: null };

/** Whether Warframe is running and logged in, from the EE.log trackers. */
export function useGame(): GameState {
  const [game, setGame] = useState<GameState>(IDLE);
  useEffect(() => {
    let live = true;
    const refresh = () => void getTrackersStatus().then((s) => live && setGame(s.game)).catch(() => {});
    refresh();
    const offs = [onGameRunning(refresh), onGameLogin(refresh)];
    return () => {
      live = false;
      offs.forEach((off) => void off.then((f) => f()));
    };
  }, []);
  return game;
}
