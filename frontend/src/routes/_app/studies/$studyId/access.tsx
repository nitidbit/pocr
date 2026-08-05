import { createFileRoute } from "@tanstack/react-router";
import { AlertCircle, RefreshCw, Trash2, UserPlus } from "lucide-react";
import { useState } from "react";
import type { StudyMember, StudyRole } from "#/api/client";
import {
	useAddMember,
	useRemoveMember,
	useStudyMembers,
	useUpdateMemberRole,
} from "#/api/queries";
import { useUser } from "#/lib/auth";
import { cn } from "#/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";

export const Route = createFileRoute("/_app/studies/$studyId/access")({
	component: StudyAccessPage,
});

const NON_OWNER_NOTE = "Only owners can manage access";

function StudyAccessPage() {
	const { studyId } = Route.useParams();
	const {
		data: members,
		isLoading,
		isError,
		refetch,
	} = useStudyMembers(studyId);
	const me = useUser();

	if (isLoading) {
		return (
			<div className="flex flex-col gap-6">
				<Skeleton className="h-32 rounded-lg" />
				<Skeleton className="h-48 rounded-lg" />
			</div>
		);
	}

	if (isError || !members) {
		return (
			<div className="flex flex-col items-center justify-center gap-4 py-16">
				<AlertCircle className="size-8 text-destructive" />
				<h2 className="text-lg font-semibold">Failed to load access list</h2>
				<Button variant="outline" onClick={() => refetch()}>
					<RefreshCw className="size-4" />
					Try again
				</Button>
			</div>
		);
	}

	// `undefined` while the caller's identity is still loading — controls treat
	// it as not-yet-disabled to avoid an owner briefly seeing disabled controls.
	const isOwner = me
		? members.some(
				(m) =>
					m.userId.toLowerCase() === me.email.toLowerCase() &&
					m.role === "owner",
			)
		: undefined;

	return (
		<div className="flex flex-col gap-6">
			<AddMemberForm studyId={studyId} isOwner={isOwner} />
			<MemberList studyId={studyId} members={members} isOwner={isOwner} />
		</div>
	);
}

function RoleSelect({
	value,
	onChange,
	disabled,
	label,
}: {
	value: StudyRole;
	onChange: (role: StudyRole) => void;
	disabled: boolean;
	label: string;
}) {
	return (
		<select
			aria-label={label}
			value={value}
			disabled={disabled}
			onChange={(e) => onChange(e.target.value as StudyRole)}
			className={cn(
				"h-8 rounded-md border border-border bg-background px-2 text-sm outline-none transition-colors",
				"focus-visible:ring-3 focus-visible:ring-ring/50",
				disabled
					? "cursor-not-allowed opacity-60"
					: "cursor-pointer hover:text-foreground",
			)}
		>
			<option value="member">Member</option>
			<option value="owner">Owner</option>
		</select>
	);
}

function AddMemberForm({
	studyId,
	isOwner,
}: {
	studyId: string;
	isOwner: boolean | undefined;
}) {
	const addMember = useAddMember(studyId);
	const [email, setEmail] = useState("");
	const [role, setRole] = useState<StudyRole>("member");

	const trimmed = email.trim();
	const isValid = trimmed !== "" && trimmed.includes("@");
	// Disable and warn only once we know the caller is not an owner; while the
	// identity is still loading (`undefined`) leave controls enabled.
	const locked = isOwner === false;

	return (
		<section className="rounded-lg border bg-card p-5">
			<h2 className="mb-1 text-sm font-medium text-muted-foreground">
				Grant Access
			</h2>
			<p className="mb-3 text-xs text-muted-foreground">
				Add a collaborator by email. Access takes effect on their next login.
			</p>
			<form
				className="flex items-end gap-2"
				onSubmit={(e) => {
					e.preventDefault();
					if (!isValid || locked) return;
					addMember.mutate(
						{ email: trimmed, role },
						{ onSuccess: () => setEmail("") },
					);
				}}
			>
				<div className="flex flex-1 flex-col gap-1">
					<label
						htmlFor="member-email"
						className="text-xs font-medium text-muted-foreground"
					>
						Email
					</label>
					<Input
						id="member-email"
						type="email"
						placeholder="collaborator@ucsf.edu"
						value={email}
						disabled={locked}
						onChange={(e) => setEmail(e.target.value)}
						className="h-8"
					/>
				</div>
				<div className="flex flex-col gap-1">
					<span className="text-xs font-medium text-muted-foreground">
						Role
					</span>
					<RoleSelect
						label="Role for new member"
						value={role}
						disabled={locked}
						onChange={setRole}
					/>
				</div>
				<Button
					type="submit"
					size="sm"
					disabled={locked || !isValid || addMember.isPending}
				>
					<UserPlus className="size-4" />
					Add
				</Button>
			</form>
			{locked && (
				<p className="mt-3 text-xs text-muted-foreground">{NON_OWNER_NOTE}</p>
			)}
		</section>
	);
}

function MemberList({
	studyId,
	members,
	isOwner,
}: {
	studyId: string;
	members: StudyMember[];
	isOwner: boolean | undefined;
}) {
	return (
		<section className="rounded-lg border bg-card p-5">
			<h2
				id="members-heading"
				className="mb-3 text-sm font-medium text-muted-foreground"
			>
				Members ({members.length})
			</h2>
			<div className="overflow-hidden rounded-md border">
				<table className="w-full text-sm" aria-labelledby="members-heading">
					<thead className="border-b bg-muted/50">
						<tr>
							<th className="px-4 py-2.5 text-left font-medium text-muted-foreground">
								Email
							</th>
							<th className="px-4 py-2.5 text-left font-medium text-muted-foreground">
								Role
							</th>
							<th className="px-4 py-2.5 text-left font-medium text-muted-foreground">
								Joined
							</th>
							<th className="px-4 py-2.5 text-right font-medium text-muted-foreground">
								<span className="sr-only">Actions</span>
							</th>
						</tr>
					</thead>
					<tbody>
						{members.map((member) => (
							<MemberRow
								key={member.userId}
								studyId={studyId}
								member={member}
								isOwner={isOwner}
							/>
						))}
					</tbody>
				</table>
			</div>
		</section>
	);
}

function MemberRow({
	studyId,
	member,
	isOwner,
}: {
	studyId: string;
	member: StudyMember;
	isOwner: boolean | undefined;
}) {
	const updateRole = useUpdateMemberRole(studyId);
	const removeMember = useRemoveMember(studyId);
	const [confirming, setConfirming] = useState(false);
	const locked = isOwner === false;

	return (
		<tr className="border-b last:border-b-0">
			<td className="px-4 py-2.5 text-foreground">{member.userId}</td>
			<td className="px-4 py-2.5">
				<RoleSelect
					label={`Role for ${member.userId}`}
					value={member.role}
					disabled={locked || updateRole.isPending}
					onChange={(role) => updateRole.mutate({ email: member.userId, role })}
				/>
			</td>
			<td className="px-4 py-2.5 text-muted-foreground">
				{new Date(member.joinedAt).toLocaleDateString()}
			</td>
			<td className="px-4 py-2.5 text-right">
				{!locked &&
					(confirming ? (
						<span className="inline-flex items-center gap-2">
							<Button
								variant="destructive"
								size="sm"
								disabled={removeMember.isPending}
								onClick={() =>
									removeMember.mutate(member.userId, {
										onSuccess: () => setConfirming(false),
									})
								}
							>
								Confirm
							</Button>
							<Button
								variant="outline"
								size="sm"
								disabled={removeMember.isPending}
								onClick={() => setConfirming(false)}
							>
								Cancel
							</Button>
						</span>
					) : (
						<Button
							variant="outline"
							size="icon"
							className="size-8"
							aria-label={`Remove ${member.userId}`}
							onClick={() => setConfirming(true)}
						>
							<Trash2 className="size-4" />
						</Button>
					))}
			</td>
		</tr>
	);
}
