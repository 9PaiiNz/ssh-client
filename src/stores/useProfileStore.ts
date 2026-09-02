import { create } from "zustand";
import type { ConnectionProfile } from "@/lib/types";
import { deleteProfile, listProfiles, saveProfile } from "@/lib/api";

interface ProfileStore {
  profiles: ConnectionProfile[];
  loading: boolean;
  loadProfiles: () => Promise<void>;
  addOrUpdateProfile: (profile: ConnectionProfile) => Promise<void>;
  removeProfile: (id: string) => Promise<void>;
}

export const useProfileStore = create<ProfileStore>((set) => ({
  profiles: [],
  loading: false,
  loadProfiles: async () => {
    set({ loading: true });
    try {
      const profiles = await listProfiles();
      set({ profiles, loading: false });
    } catch {
      set({ loading: false });
    }
  },
  addOrUpdateProfile: async (profile) => {
    await saveProfile(profile);
    const profiles = await listProfiles();
    set({ profiles });
  },
  removeProfile: async (id) => {
    await deleteProfile(id);
    set((state) => ({
      profiles: state.profiles.filter((p) => p.id !== id),
    }));
  },
}));
