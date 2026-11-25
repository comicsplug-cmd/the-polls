// Read-only Vercel showcase fallback. Keep this in sync with data/seed/elections.json.
type CandidateTuple = [party: string, name: string, runningMate: string];

const presidentialTickets: CandidateTuple[] = [
  ["AA", "Rufai Adekunle F Omo-Aje", "Shehu Hussaini"],
  ["AAC", "Sowore Omoyele", "Magashi Haruna Garba"],
  ["ADC", "Abubakar Atiku", "Amaechi Chibuike Rotimi"],
  ["ADP", "Abbas-Bin Aliyu", "Ike Chinazam"],
  ["APC", "Tinubu Bola Ahmed", "Shettima Kashim"],
  ["APM", "Makinde Oluseyi Abiodun", "Musa Lawal Daura"],
  ["APP", "Yusuf Kabiru", "Ofordile Peace Egobia"],
  ["BP", "Adenuga Sunday", "Mustapha Usman Turaki"],
  ["DLA", "Adebisi Moses Olusoji", "Abubakar Nafisat Usaku"],
  ["LP", "Okereke Sunday Chibuzo", "Konto Hajja Bintu"],
  ["NDC", "Obi Peter Gregory", "Musa Mohammed Rabiu Kwankwaso"],
  ["NDP", "Okwori Ada Elizabeth Fredrick", "Chukwuemeka Uchenna Anthony"],
  ["NRM", "Okereke Nkem Esther", "Sulaiman Nasir Muhammed"],
  ["PDP", "Onor Sandy Ojang", "Umaru Babangida"],
  ["PRP", "Duke Donald", "Mohammed Sule"],
  ["SDP", "Adebayo Adewole Ebenezer", "Bugaje Usman Muhammed"],
  ["YPP", "Agada Peter Ada", "Key Patience Ndidi"],
  ["ZLP", "Nwanyanwu Daniel Daberechukwu", "Hassan Khalid"],
];

export const previewElections = [
  {
    slug: "president-2027",
    title: "President of Nigeria",
    region: "Nigeria",
    election_date: "2027-01-16",
    open_at: "2026-12-26T00:00:00+01:00",
    close_at: "2027-01-04T23:59:59+01:00",
    included: true,
    roster_status: "pending_final_legal_and_qa",
    source_url: "https://www.inecnigeria.org/documents/uploads/2026/10/b5af9099-864c-4d5e-924d-e1a2c771c529-final-list-presidential.pdf",
    source_label: "INEC updated final presidential candidate list",
    is_open: false,
    candidates: presidentialTickets.map(([party, name, running_mate], index) => ({ id: index + 1, party, name, running_mate })),
  },
  {
    slug: "rivers-governor-2027",
    title: "Governor of Rivers State",
    region: "Rivers State",
    election_date: "2027-02-06",
    open_at: "2026-12-26T00:00:00+01:00",
    close_at: "2027-01-04T23:59:59+01:00",
    included: false,
    roster_status: "pending_transcription_and_qa",
    source_url: "https://www.inecnigeria.org/documents/uploads/2026/10/81d476df-5101-426b-af72-8e493dba1cb2-final-list-rivers-governorship.pdf",
    source_label: "INEC updated final Rivers governorship candidate list",
    is_open: false,
    candidates: [],
  },
];

export function previewTally(slug: string) {
  const election = previewElections.find((row) => row.slug === slug);
  if (!election) return null;
  return {
    election: election.title,
    total_counted: 0,
    updated_at: new Date().toISOString(),
    preview_only: true,
    results: election.candidates.map((candidate) => ({
      party: candidate.party,
      name: candidate.name,
      running_mate: candidate.running_mate,
      total: 0,
    })),
  };
}
